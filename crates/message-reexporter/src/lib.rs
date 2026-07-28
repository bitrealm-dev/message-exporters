//! Convert an existing Message Exporters output directory to another IR format.
//!
//! Auto-detects csv / eml / mbox / json / jsonl / xml, loads conversations into
//! IR, then writes via [`message_ir::FormatSink`].

mod convert;
mod detect;
pub mod run;

pub use convert::{convert_export, ReexportReport};
pub use detect::{detect_ir_export, DetectedExport};
pub use message_exporters_core::{ExporterConfig, OutputFormat};
pub use run::{run, RunResult};
