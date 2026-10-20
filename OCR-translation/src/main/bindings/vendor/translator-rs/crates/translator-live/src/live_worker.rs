//! Single-slot async worker shared by the live pipelines (camera tracker and
//! screen capture). At most one job is pending or in flight at a time; a
//! dispatch while busy is dropped, which is the pipelines' per-frame
//! backpressure — the caller retries on a later frame. The thread is spawned
//! once for the pipeline's lifetime and drained via a `Condvar`, so we don't pay
//! thread-spawn cost per acquire. Teardown is driven by `Drop`.
//!
//! The `handler` closure captures a `Weak` back-reference to its pipeline and
//! upgrades it per job, no-opping if the pipeline is gone; the thread then exits
//! on its next loop via the shutdown flag that `Drop` sets. Keeping the capture
//! `Weak` is what lets the pipeline own the worker without a reference cycle.

use std::sync::{Arc, Condvar, Mutex};

struct SlotState<J> {
    pending: Option<J>,
    busy: bool,
    shutting_down: bool,
}

struct SlotInner<J> {
    slot: Mutex<SlotState<J>>,
    cv: Condvar,
}

pub(crate) struct SlotWorker<J> {
    inner: Arc<SlotInner<J>>,
}

impl<J: Send + 'static> SlotWorker<J> {
    /// Spawn the worker thread (named `name`) running `handler` on each
    /// dispatched job. `handler` runs off the calling thread; keep its captures
    /// `Weak` so the worker doesn't extend the pipeline's lifetime.
    pub(crate) fn spawn<F>(name: &str, handler: F) -> Self
    where
        F: Fn(J) + Send + 'static,
    {
        let inner = Arc::new(SlotInner {
            slot: Mutex::new(SlotState {
                pending: None,
                busy: false,
                shutting_down: false,
            }),
            cv: Condvar::new(),
        });
        let inner_clone = Arc::clone(&inner);
        std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                loop {
                    let job = {
                        let mut state = inner_clone.slot.lock().expect("worker slot poisoned");
                        while state.pending.is_none() && !state.shutting_down {
                            state = inner_clone.cv.wait(state).expect("worker cv poisoned");
                        }
                        if state.shutting_down {
                            return;
                        }
                        let job = state.pending.take();
                        if job.is_some() {
                            state.busy = true;
                        }
                        job
                    };
                    let Some(job) = job else { continue };
                    handler(job);
                    let mut state = inner_clone.slot.lock().expect("worker slot poisoned");
                    state.busy = false;
                    inner_clone.cv.notify_all();
                }
            })
            .expect("failed to spawn worker thread");
        SlotWorker { inner }
    }

    /// Dispatch `job`, returning whether it was accepted. Drops the job
    /// (returns false) when one is already pending or in flight.
    pub(crate) fn try_dispatch(&self, job: J) -> bool {
        let mut state = self.inner.slot.lock().expect("worker slot poisoned");
        if state.busy || state.pending.is_some() {
            return false;
        }
        state.pending = Some(job);
        self.inner.cv.notify_one();
        true
    }

    /// Whether a job is pending or in flight.
    pub(crate) fn busy(&self) -> bool {
        let state = self.inner.slot.lock().expect("worker slot poisoned");
        state.busy || state.pending.is_some()
    }
}

impl<J> Drop for SlotWorker<J> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.inner.slot.lock() {
            state.shutting_down = true;
            self.inner.cv.notify_all();
        }
    }
}
