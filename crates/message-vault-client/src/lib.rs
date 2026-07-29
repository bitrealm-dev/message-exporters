//! Push message-ir JSONL export folders into a Message Vault import API.
//!
//! Used by the `vault-push` CLI and the Message Exporters GUI Vault tab.

mod http;
mod journal;
mod project;
mod run;

pub use http::AuthInfo;
pub use journal::{JOURNAL_NAME, LOG_NAME, REPORT_NAME};
pub use run::{
    DEFAULT_ASSET_UPLOAD_WORKERS, DEFAULT_BATCH_SIZE, FileResult, ProgressEvent, ProgressFn,
    PushReport, VaultPushConfig, authenticate, detect_source, run,
};
