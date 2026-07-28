//! iMessage → per-conversation CSV / EML / MBOX / JSON via `imessage-database`.
//!
//! Messages stream from `chat.db`, build [`message_mail::MailMessage`] per row,
//! convert to canonical [`message_ir::IrMessage`], and project via
//! [`message_ir::write_format`].

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
