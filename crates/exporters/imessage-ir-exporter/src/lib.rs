//! iMessage → per-conversation CSV / EML / MBOX / JSON / JSONL / XML via `imessage-database`.
//!
//! Messages stream from `chat.db`, build [`mail::MailMessage`] per row,
//! convert to canonical [`ir::IrMessage`], and project via
//! [`ir::FormatSink`].

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

pub use run::{RunResult, run};
