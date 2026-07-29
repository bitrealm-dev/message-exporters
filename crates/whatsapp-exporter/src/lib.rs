//! WhatsApp (via KnugiHK wtsexporter JSON) → shared per-chat CSV.
//!
//! Library entrypoints: [`run`] with [`ExporterConfig`] for the full pipeline
//! (wtsexporter/JSON convert + media + obfuscate), or [`convert_json`] for convert-only.
//! The `whatsapp-exporter` binary is a thin CLI over [`run`].

mod emit;
mod jid;
mod parse;
pub mod run;
mod wtsexporter;

pub use emit::{ExportReport, convert_json};
pub use jid::{chat_id_from_jid, is_group_jid, jid_to_e164};
pub use message_exporters_core::{CancelFlag, ExporterConfig, is_cancelled};
pub use parse::{ChatStoreFile, load_chat_store};
pub use run::{RunResult, parse_date_range, report_summary_lines, run};
pub use wtsexporter::{Platform, WtsexporterArgs, resolve_wtsexporter, run_wtsexporter};
