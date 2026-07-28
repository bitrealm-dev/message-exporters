//! SMS Backup & Restore → per-conversation CSV or EML archive exporter.
//!
//! Library entrypoints: [`run`] with [`ExporterConfig`] for the full pipeline
//! (convert + media + obfuscate), or [`convert_export`] for convert-only.
//! Set [`ExporterConfig::output_format`] to [`OutputFormat::Eml`] for mail
//! archive folders. The `sms-backup-restore-exporter` binary is a thin CLI over
//! [`run`] (`--format eml`).

pub(crate) mod assets;
pub(crate) mod cancel;
pub(crate) mod emit;
pub mod run;
pub(crate) mod smil;
pub(crate) mod xml;

pub use cancel::{is_cancelled, CancelFlag};
pub use emit::{
    convert_export, infer_owner_phones_from_xml, load_documents_from_xml, ExportReport,
};
pub use message_exporters_core::{ExporterConfig, OutputFormat};
pub use run::{parse_date_range, report_summary_lines, run, RunResult};
