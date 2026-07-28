//! SMS Backup & Restore → per-conversation CSV exporter.
//!
//! Library entrypoints: [`run`] with [`ExportConfig`] for the full pipeline
//! (convert + media + obfuscate), or [`convert_export`] for convert-only.
//! The `sms-backup-restore-exporter` binary is a thin CLI over [`run`].

pub(crate) mod assets;
pub(crate) mod cancel;
pub(crate) mod emit;
pub mod run;
pub(crate) mod smil;
pub(crate) mod xml;

pub use cancel::{is_cancelled, CancelFlag};
pub use emit::{convert_export, ExportReport};
pub use run::{
    parse_date_range, report_summary_lines, run, ExportConfig, RunResult,
};
