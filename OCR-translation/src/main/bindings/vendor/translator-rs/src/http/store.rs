use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

/// Holds `/translate_file` outputs until a client fetches them via
/// `/download/<id>`. Outputs older than [`TTL`] are swept on each new upload,
/// including files left behind by a previous process, so nothing accumulates
/// unbounded.
pub struct FileStore {
    dir: PathBuf,
    entries: Mutex<HashMap<String, Entry>>,
}

#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub download_name: String,
    pub mime: &'static str,
    created: Instant,
}

const TTL: Duration = Duration::from_secs(60 * 60);

impl FileStore {
    pub fn open(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            entries: Mutex::new(HashMap::new()),
        })
    }

    pub fn put(
        &self,
        bytes: &[u8],
        extension: &str,
        download_name: String,
        mime: &'static str,
    ) -> io::Result<String> {
        self.sweep();
        let id = format!("{:032x}", rand::random::<u128>());
        let path = self.dir.join(format!("{id}.{extension}"));
        fs::write(&path, bytes)?;
        self.entries.lock().expect("file store lock").insert(
            id.clone(),
            Entry {
                path,
                download_name,
                mime,
                created: Instant::now(),
            },
        );
        Ok(id)
    }

    pub fn get(&self, id: &str) -> Option<Entry> {
        self.entries
            .lock()
            .expect("file store lock")
            .get(id)
            .cloned()
    }

    fn sweep(&self) {
        let now = Instant::now();
        self.entries
            .lock()
            .expect("file store lock")
            .retain(|_, entry| now.duration_since(entry.created) < TTL);

        let Ok(listing) = fs::read_dir(&self.dir) else {
            return;
        };
        let cutoff = SystemTime::now() - TTL;
        for file in listing.flatten() {
            let expired = file
                .metadata()
                .and_then(|meta| meta.modified())
                .is_ok_and(|modified| modified < cutoff);
            if expired {
                let _ = fs::remove_file(file.path());
            }
        }
    }
}
