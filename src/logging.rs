use std::fs;
use std::path::Path;
use std::str::FromStr;
use ftail::Ftail;
use log::LevelFilter;
use crate::config::PPConfig;

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

#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        log::trace!(target: "pproxy", $($arg)*)
    };
}

pub fn init_log(conf : PPConfig) {
    let log_level = LevelFilter::from_str(&conf.log_level).unwrap();
    fs::create_dir(conf.log_path.clone()).unwrap();
    let logger = Ftail::new()
        //TODO dev prod profiles
        //.console(log_level)
        .daily_file(Path::new(&conf.log_path), log_level)
        .max_file_size(10)
        .retention_days(30);
    let logger = if conf.log_groups.is_empty() {
        logger
    } else {
        let log_group_targets: Vec<&str> = conf.log_groups.iter().map(String::as_str).collect();
        logger.filter_targets(log_group_targets)
    };
    let _ = logger.init();
}