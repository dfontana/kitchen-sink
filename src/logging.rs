use anyhow::{anyhow, bail};
use std::sync::{LazyLock, Mutex};
use tracing::Level;
use tracing_error::ErrorLayer;
use tracing_subscriber::{
    Registry,
    filter::LevelFilter,
    fmt::Layer,
    layer::SubscriberExt,
    prelude::*,
    registry::LookupSpan,
    reload::{self, Handle},
};

// Global handle for runtime log level changes
static LOG_RELOAD_HANDLE: LazyLock<Mutex<Option<Handle<LevelFilter, Registry>>>> =
    LazyLock::new(|| Mutex::new(None));

pub fn set_log_level(level: Level) -> Result<(), anyhow::Error> {
    let handle_guard = LOG_RELOAD_HANDLE
        .lock()
        .map_err(|e| anyhow!("Lock error: {}", e))?;
    if let Some(handle) = handle_guard.as_ref() {
        handle
            .modify(|filter| *filter = LevelFilter::from_level(level))
            .map_err(|e| anyhow!("Failed to update log level: {}", e))?;
        Ok(())
    } else {
        bail!("Log reload handle not initialized")
    }
}

/// Builds a logging subscriber with reload and error layers, but no output layer.
///
/// This function allows callers to add their own output layers (such as file appenders,
/// syslog, or custom formatters) before initializing the subscriber.
///
/// The subscriber includes:
/// - A reload layer for runtime log level changes via `set_log_level()`
/// - An error layer for error tracking
/// - **No output layer** - callers must add their own
///
/// # Example with stdout
/// ```no_run
/// use kitchen_sink::logging::build_logging_subscriber;
/// use tracing_subscriber::{fmt::Layer, prelude::*};
///
/// build_logging_subscriber()
///     .with(Layer::default())
///     .init();
/// ```
///
/// # Example with RollingFileAppender
/// ```no_run
/// use kitchen_sink::logging::build_logging_subscriber;
/// use tracing_appender::rolling::{RollingFileAppender, Rotation};
/// use tracing_subscriber::{fmt::Layer, prelude::*};
///
/// let file_appender = RollingFileAppender::builder()
///     .max_log_files(5)  // Keep last 5 files based on size
///     .rotation(Rotation::HOURLY)
///     .filename_prefix("app")
///     .build("/var/log/myapp")
///     .unwrap();
///
/// build_logging_subscriber()
///     .with(Layer::default().with_writer(file_appender))
///     .init();
/// ```
pub fn build_logging_subscriber()
-> impl tracing::Subscriber + for<'a> LookupSpan<'a> + Send + Sync + 'static {
    let (filter, reload_handle) = reload::Layer::new(LevelFilter::from_level(Level::INFO));
    {
        let mut handle_guard = LOG_RELOAD_HANDLE.lock().unwrap();
        *handle_guard = Some(reload_handle);
    }
    tracing_subscriber::Registry::default()
        .with(filter)
        .with(ErrorLayer::default())
}

/// Initializes logging with default stdout output.
///
/// This is a convenience function that sets up logging with:
/// - Runtime log level changes via `set_log_level()`
/// - Error tracking
/// - Stdout output with target disabled
///
/// For custom output layers (file, syslog, etc.), use `build_logging_subscriber()` instead.
///
/// # Example
/// ```no_run
/// use kitchen_sink::logging::initialize_logging;
/// initialize_logging();
/// ```
pub fn initialize_logging() {
    build_logging_subscriber()
        .with(Layer::default().with_target(false))
        .init();
}
