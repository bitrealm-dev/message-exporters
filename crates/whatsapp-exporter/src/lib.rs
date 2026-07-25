//! WhatsApp (via KnugiHK wtsexporter JSON) → per-conversation vault-shaped CSV.

mod emit;
mod jid;
mod parse;
mod wtsexporter;

pub use emit::{convert_json, ExportReport};
pub use jid::{chat_id_from_jid, is_group_jid, jid_to_e164};
pub use parse::{load_chat_store, ChatStoreFile};
pub use wtsexporter::{
    resolve_wtsexporter, run_wtsexporter, Platform, WtsexporterArgs,
};
