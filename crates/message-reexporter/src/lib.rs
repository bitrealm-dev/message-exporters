//! Convert an existing Message Exporters output directory to another IR format.
//!
//! Auto-detects csv / eml / mbox / json / jsonl / xml, loads conversations into
//! IR, then writes via [`message_ir::FormatSink`].

mod convert;
mod detect;
pub mod run;

pub use convert::{ReexportReport, convert_export};
pub use detect::{DetectedExport, detect_ir_export};
pub use message_exporters_core::{ExporterConfig, OutputFormat};
pub use run::{RunResult, run};
