//! WhatsApp (via KnugiHK wtsexporter JSON) → shared per-chat CSV.
//!
//! Library entrypoint: [`run`] for the full pipeline.
//! The `whatsapp-exporter` binary is a thin CLI over [`run`].

mod emit;
mod jid;
mod parse;
mod run;
mod wtsexporter;

pub use run::{RunResult, parse_date_range, run};

#[cfg(test)]
#[path = "../tests/convert_smoke.rs"]
mod convert_smoke;
