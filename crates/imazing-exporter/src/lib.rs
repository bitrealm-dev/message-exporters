//! iMazing Messages / WhatsApp CSV (+ Contacts CSV) → per-conversation vault-shaped CSV.
//!
//! Library entrypoints: [`run`] with [`ExporterConfig`] for the full pipeline
//! (convert + media + obfuscate), or [`convert_export`] for convert-only.
//! The `imazing-exporter` binary is a thin CLI over [`run`].

pub(crate) mod cancel;
mod emit;
mod parse;
pub mod run;

pub use cancel::{is_cancelled, CancelFlag};
pub use emit::{convert_export, ExportReport};
pub use message_exporters_core::ExporterConfig;
pub use run::{parse_date_range, report_summary_lines, run, RunResult};
