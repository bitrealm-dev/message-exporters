//! OpenExtract conversation CSV (+ VCF) → per-conversation vault-shaped CSV.

mod emit;
mod parse;

pub use emit::{convert_export, ExportReport};
