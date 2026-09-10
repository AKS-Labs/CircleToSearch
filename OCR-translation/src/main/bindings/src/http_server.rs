use std::sync::{Arc, Mutex};

use thiserror::Error;
use translator::TranslatorSession;
use translator::http::{HttpServer, HttpServerConfig, SessionSource};

use crate::uniffi_catalog::CatalogHandle;

/// Hands out the catalog to serve a request with. The app re-opens its
/// catalog after downloads and migrations, so the server asks per request
/// instead of holding the one it was started with.
#[uniffi::export(with_foreign)]
pub trait HttpSessionSource: Send + Sync {
    fn catalog(&self) -> Option<Arc<CatalogHandle>>;
}

struct ForeignSessionSource(Arc<dyn HttpSessionSource>);

impl SessionSource for ForeignSessionSource {
    fn session(&self) -> Option<Arc<TranslatorSession>> {
        self.0.catalog().map(|catalog| catalog.session.clone())
    }
}

#[derive(Debug, Error, uniffi::Error)]
pub enum HttpServerError {
    #[error("{reason}")]
    Start { reason: String },
}

#[derive(uniffi::Object)]
pub struct HttpServerHandle {
    server: Mutex<Option<HttpServer>>,
}

#[uniffi::export]
impl HttpServerHandle {
    /// Closes the socket and waits for in-flight requests to finish. A no-op
    /// after the first call.
    fn stop(&self) {
        let server = self.server.lock().expect("http server lock").take();
        if let Some(server) = server {
            server.stop();
        }
    }
}

#[uniffi::export]
pub fn start_http_server(
    config: HttpServerConfig,
    sessions: Arc<dyn HttpSessionSource>,
) -> Result<Arc<HttpServerHandle>, HttpServerError> {
    crate::init_logging();
    let server = translator::http::start(
        config,
        Arc::new(ForeignSessionSource(sessions)),
        Arc::new(crate::android_font_provider::AndroidFontProvider),
    )
    .map_err(|error| HttpServerError::Start {
        reason: error.to_string(),
    })?;
    Ok(Arc::new(HttpServerHandle {
        server: Mutex::new(Some(server)),
    }))
}
