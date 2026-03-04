use crate::config::RPConfig;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::registry::LookupSpan;

struct NiceFormat;

impl<S, N> FormatEvent<S, N> for NiceFormat
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut w: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let meta = event.metadata();

        let (sigil, rite) = match *meta.level() {
            tracing::Level::ERROR => ("☠", "RITE:ABORT"),
            tracing::Level::WARN => ("⚠", "RITE:CAUTION"),
            tracing::Level::INFO => ("⚙", "RITE:STATUS"),
            tracing::Level::DEBUG => ("⛭", "RITE:DIAGNOSTIC"),
            tracing::Level::TRACE => ("⌁", "RITE:TELEMETRY"),
        };

        write!(
            w,
            "⟦⚙ NOOSPHERE·DATALOG ⟧ {} ⟦{}⟧ ⟦FORGE:{}⟧ ",
            sigil,
            rite,
            meta.target()
        )?;

        if let Some(scope) = ctx.event_scope() {
            write!(w, "⟦SUBROUTINE:")?;
            let mut first = true;
            for span in scope.from_root() {
                if !first {
                    write!(w, "→")?;
                }
                first = false;
                write!(w, "{}", span.name())?;
            }
            write!(w, "⟧ ")?;
        }

        write!(w, "⟦DATA⟧ ")?;
        ctx.field_format().format_fields(w.by_ref(), event)?;

        writeln!(w)
    }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        tracing::info!(target: "rproxy", "++++++++++++ {}", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        tracing::error!(target: "rproxy", "++++++++++++ {}", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        tracing::warn!(target: "rproxy", "++++++++++++ {}", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        tracing::trace!(target: "rproxy", "++++++++++++ {}", format_args!($($arg)*))
    };
}

pub fn init_tracing(conf: RPConfig) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(conf.log_level));

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
