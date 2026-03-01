use tracing_subscriber::{fmt, EnvFilter};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::config::RPConfig;

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        tracing::info!(target: "rproxy", $($arg)*)
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        tracing::error!(target: "rproxy", $($arg)*)
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        tracing::warn!(target: "rproxy", $($arg)*)
    };
}

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        tracing::trace!(target: "rproxy", $($arg)*)
    };
}

pub fn init_tracing(conf : RPConfig) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(conf.log_level));

    // Dev
    #[cfg(debug_assertions)]
    {
        let stdout_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_thread_names(true);

        tracing_subscriber::registry()
            .with(filter)
            .with(stdout_layer)
            .init();

        None
    }

    // Prod
    #[cfg(not(debug_assertions))]
    {
        let file_appender = tracing_appender::rolling::daily(conf.log_path, "rproxy");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let file_layer = fmt::layer()
            .with_ansi(false)
            .with_target(true)
            .with_writer(non_blocking);

        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .init();

        Some(guard) // keep this alive for the lifetime of the program!
    }
}