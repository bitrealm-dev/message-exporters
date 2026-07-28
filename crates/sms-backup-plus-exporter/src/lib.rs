//! SMS Backup+ (jberkel) EML → per-conversation CSV exporter.
//!
//! Library entrypoints: [`run`] with [`ExporterConfig`] for the full pipeline
//! (convert + media + obfuscate), or [`convert_export`] for convert-only.
//! The `sms-backup-plus-exporter` binary is a thin CLI over [`run`].

pub(crate) mod archive;
pub(crate) mod assets;
pub(crate) mod cancel;
pub(crate) mod contacts;
pub(crate) mod emit;
pub(crate) mod flat_eml;
pub(crate) mod identity;
pub mod run;
pub(crate) mod types;

pub use cancel::{is_cancelled, CancelFlag};
pub use emit::{ExportReport, convert_export};
pub use message_exporters_core::ExporterConfig;
pub use run::{
    parse_date_range, report_summary_lines, resolve_inputs, resolve_owner, run, RunResult,
};
