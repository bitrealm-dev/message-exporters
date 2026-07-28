//! WhatsApp (via KnugiHK wtsexporter JSON) → per-conversation vault-shaped CSV.
//!
//! Library entrypoints: [`run`] with [`ExportConfig`] for the full pipeline
//! (wtsexporter/JSON convert + media + obfuscate), or [`convert_json`] for convert-only.
//! The `whatsapp-exporter` binary is a thin CLI over [`run`].

pub(crate) mod cancel;
mod emit;
mod jid;
mod parse;
pub mod run;
mod wtsexporter;

pub use cancel::{is_cancelled, CancelFlag};
pub use emit::{convert_json, ExportReport};
pub use jid::{chat_id_from_jid, is_group_jid, jid_to_e164};
pub use parse::{load_chat_store, ChatStoreFile};
pub use run::{parse_date_range, report_summary_lines, run, ExportConfig, RunResult};
pub use wtsexporter::{
    resolve_wtsexporter, run_wtsexporter, Platform, WtsexporterArgs,
};
