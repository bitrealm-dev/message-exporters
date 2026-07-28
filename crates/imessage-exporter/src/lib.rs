#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

pub mod app;
pub mod cancel;
pub mod exporters;
pub mod run;

pub use app::options::Options;
pub use app::runtime::Config;
pub use cancel::{is_cancelled, CancelFlag};
pub use exporters::{csv::CSV, html::HTML, txt::TXT};
pub use run::{options_from_export_config, run, run_with_options, ExportConfig, RunResult};
