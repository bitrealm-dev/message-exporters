//! iMazing Messages / WhatsApp CSV (+ Contacts CSV) → per-conversation vault-shaped CSV.

mod emit;
mod parse;

pub use emit::{convert_export, ExportReport};
