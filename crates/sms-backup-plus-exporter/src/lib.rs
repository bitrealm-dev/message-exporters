//! SMS Backup+ (jberkel) EML → SMS NDJSON (`message_json::sms`) exporter.

pub(crate) mod archive;
pub(crate) mod assets;
pub(crate) mod contacts;
pub(crate) mod dedupe_eml;
pub(crate) mod emit;
pub(crate) mod flat_eml;
pub(crate) mod identity;
pub(crate) mod phone;
pub(crate) mod types;
pub(crate) mod write_flat_eml;

pub use dedupe_eml::{DedupeReport, dedupe_eml};
pub use emit::{ExportReport, convert_export};
