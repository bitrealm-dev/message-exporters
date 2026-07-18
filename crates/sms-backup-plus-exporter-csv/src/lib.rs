//! SMS Backup+ (jberkel) EML → per-conversation CSV exporter.

pub(crate) mod archive;
pub(crate) mod assets;
pub(crate) mod contacts;
pub(crate) mod emit;
pub(crate) mod flat_eml;
pub(crate) mod identity;
pub(crate) mod phone;
pub(crate) mod types;

pub use emit::{ExportReport, convert_export};
