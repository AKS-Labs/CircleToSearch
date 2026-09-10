uniffi::setup_scaffolding!();

pub mod adblock;
pub mod android_font_provider;
pub mod bergamot;
pub mod http_server;
pub mod selection;
pub mod transliterate;
pub mod uniffi_catalog;

#[cfg(all(feature = "planar-tracker", target_os = "android"))]
mod live_pipeline_jni;

#[cfg(target_os = "android")]
mod bitmap_jni;

/// Idempotent logger init. Called from the first uniffi entry point
/// (`CatalogHandle::open`) because uniffi 0.29 for Kotlin loads the
/// .so via JNA, which does *not* invoke `JNI_OnLoad` — so a JNI-style
/// init never fires and `log::set_max_level` stays at its default
/// `Off`, silently dropping every `log::*!` call. Calling it on every
/// constructor invocation is safe (`init_once` is idempotent and skips
/// after the first successful registration).
#[cfg(target_os = "android")]
pub(crate) fn init_logging() {
    // Default Info, but Debug for our own crates. Without the
    // per-target filter, third-party crates like `xml5ever` flood the
    // log at Debug ("got character l" per char) and bury anything
    // useful.
    let filter = android_logger::FilterBuilder::new()
        .parse("info,translator=info,bindings=info")
        .build();
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_filter(filter)
            .with_tag("rust-bindings"),
    );
    std::panic::set_hook(Box::new(|info| {
        log::error!("rust panic: {info}");
    }));
}

#[cfg(not(target_os = "android"))]
pub(crate) fn init_logging() {}
