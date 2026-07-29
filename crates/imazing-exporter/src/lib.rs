//! iMazing Messages / WhatsApp CSV (+ Contacts CSV) → shared per-chat CSV.
//!
//! Library entrypoints: [`run`] with [`ExporterConfig`] for the full pipeline
//! (convert + media + obfuscate), or [`convert_export`] for convert-only.
//! The `imazing-exporter` binary is a thin CLI over [`run`].

pub(crate) mod cancel;
mod emit;
mod parse;
pub mod run;

pub use cancel::{CancelFlag, is_cancelled};
pub use emit::{ExportReport, convert_export};
pub use message_exporters_core::ExporterConfig;
pub use run::{RunResult, parse_date_range, report_summary_lines, run};
