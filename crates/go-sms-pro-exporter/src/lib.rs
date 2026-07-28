//! GO SMS Pro → per-conversation CSV exporter.
//!
//! Library entrypoints: [`run`] with [`ExporterConfig`] for the full pipeline
//! (convert + media + obfuscate), or [`convert_export`] for convert-only.
//! The `go-sms-pro-exporter` binary is a thin CLI over [`run`].

pub(crate) mod cancel;
pub(crate) mod emit;
pub(crate) mod emoji;
pub(crate) mod mms_enc;
pub(crate) mod pdu;
pub(crate) mod phone;
pub mod run;
pub(crate) mod xml;

pub use cancel::{is_cancelled, CancelFlag};
pub use emit::{convert_export, ExportReport};
pub use message_exporters_core::ExporterConfig;
pub use run::{parse_date_range, report_summary_lines, run, RunResult};
