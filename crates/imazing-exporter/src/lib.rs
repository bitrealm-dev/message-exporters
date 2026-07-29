//! iMazing Messages / WhatsApp CSV (+ Contacts CSV) → shared per-chat CSV.
//!
//! Library entrypoint: [`run`] for the full pipeline.
//! The `imazing-exporter` binary is a thin CLI over [`run`].

mod attachments;
mod emit;
mod parse;
mod run;

pub use run::{RunResult, parse_date_range, run};

#[cfg(test)]
#[path = "../tests/convert_smoke.rs"]
mod convert_smoke;
