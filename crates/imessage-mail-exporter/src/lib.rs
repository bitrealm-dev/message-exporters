//! iMessage → per-conversation `.eml` mail archives via `imessage-database`.
//!
//! Does **not** depend on `imessage-exporter`. CSV export remains on that crate;
//! GUI/library callers route `OutputFormat::Eml` / [`OutputFormat::Mbox`] here.

mod attachments;
mod backup;
mod body;
mod contacts;
mod data_source;
mod emit;
mod error;
mod fields;
mod options;
mod run;
mod session;

pub use error::RuntimeError;
pub use message_exporters_core::{ExporterConfig, OutputFormat};
pub use run::{run, RunResult};
