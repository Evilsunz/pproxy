#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        log::info!(target: "pproxy", $($arg)*)
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        log::error!(target: "pproxy", $($arg)*)
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        log::warn!(target: "pproxy", $($arg)*)
    };
}