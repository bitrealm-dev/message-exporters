//! SMS Backup+ (jberkel) EML → per-conversation CSV exporter.
//!
//! Library entrypoint: [`run`] for the full pipeline.
//! The `sms-backup-plus-exporter` binary is a thin CLI over [`run`].

mod archive;
mod assets;
mod contacts;
mod emit;
mod flat_eml;
mod identity;
mod run;
mod types;

pub use run::{RunResult, parse_date_range, run};

#[cfg(test)]
#[path = "../tests/convert_smoke.rs"]
mod convert_smoke;
