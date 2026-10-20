//! The subset of the LibreTranslate HTTP API that maps onto the on-device
//! engine: `POST /translate`, `POST /detect`, `GET /languages`,
//! `POST /translate_file` + `GET /download/<id>`, and a bundled web page at
//! `/`. Any client that already speaks LibreTranslate can point at it
//! unchanged. Hosts own the lifecycle: they call [`start`] with a config
//! snapshot and [`HttpServer::stop`] when the config changes or the app exits.

mod api;
mod store;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;

use crate::font_provider::FontProvider;
use crate::{BackgroundMode, TranslatorSession};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum BindAddress {
    Localhost,
    AllInterfaces,
}

impl BindAddress {
    fn ip(self) -> IpAddr {
        match self {
            BindAddress::Localhost => IpAddr::V4(Ipv4Addr::LOCALHOST),
            BindAddress::AllInterfaces => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct OcrSettings {
    pub max_image_size: u32,
    pub min_confidence: u32,
    pub background_mode: BackgroundMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct HttpServerConfig {
    pub bind: BindAddress,
    pub port: u16,
    /// Where `/translate_file` outputs wait for their `/download/<id>` fetch.
    pub output_dir: String,
    pub ocr: OcrSettings,
    pub translate_pdf_images: bool,
}

/// Resolves the session to serve a request with. Hosts that re-open their
/// catalog (a new session per reload) hand out the current one per call, so
/// the server never serves a stale availability snapshot.
pub trait SessionSource: Send + Sync {
    fn session(&self) -> Option<Arc<TranslatorSession>>;
}

impl<F> SessionSource for F
where
    F: Fn() -> Option<Arc<TranslatorSession>> + Send + Sync,
{
    fn session(&self) -> Option<Arc<TranslatorSession>> {
        self()
    }
}

#[derive(Debug)]
pub struct StartError(pub String);

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StartError {}

pub struct HttpServer {
    addr: SocketAddr,
    stop: mpsc::Sender<()>,
    thread: JoinHandle<()>,
}

impl HttpServer {
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Closes the listening socket and waits for in-flight requests to finish.
    pub fn stop(self) {
        let _ = self.stop.send(());
        if self.thread.join().is_err() {
            log::error!("http server thread panicked");
        }
        log::info!("http server on {} stopped", self.addr);
    }
}

// The engine serialises translations anyway, so a small pool bounds the
// threads a burst of clients can pin on a phone.
const WORKER_THREADS: usize = 4;

pub fn start(
    config: HttpServerConfig,
    sessions: Arc<dyn SessionSource>,
    fonts: Arc<dyn FontProvider + Send + Sync>,
) -> Result<HttpServer, StartError> {
    let addr = SocketAddr::new(config.bind.ip(), config.port);
    let store = store::FileStore::open(config.output_dir.as_ref())
        .map_err(|error| StartError(format!("cannot use output dir: {error}")))?;
    let api = api::Api::new(config, sessions, fonts, store);
    let server = rouille::Server::new(addr, move |request| api.handle(request))
        .map_err(|error| StartError(error.to_string()))?
        .pool_size(WORKER_THREADS);
    let addr = server.server_addr();
    let (thread, stop) = server.stoppable();
    log::info!("http server listening on {addr}");
    Ok(HttpServer { addr, stop, thread })
}
