//! OpenExtract conversation CSV (+ VCF) → per-conversation vault-shaped CSV.
//!
//! Library entrypoints: [`run`] with [`ExportConfig`] for the full pipeline
//! (convert + obfuscate), or [`convert_export`] for convert-only.
//! The `openextract-exporter` binary is a thin CLI over [`run`].

pub(crate) mod cancel;
pub(crate) mod emit;
pub(crate) mod parse;
pub mod run;

pub use cancel::{is_cancelled, CancelFlag};
pub use emit::{convert_export, ExportReport};
pub use run::{
    parse_date_range, report_summary_lines, run, ExportConfig, RunResult,
};
