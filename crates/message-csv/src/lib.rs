//! Shared CSV emit helpers for message exporters.

mod date_range;
mod utc_offset;

pub use date_range::DateRange;
pub use utc_offset::parse_utc_offset;

use chrono::{Local, TimeZone, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// One attachment object written into `attachments_json`.
#[derive(Debug, Serialize)]
pub struct AttachmentCell {
    pub path: Option<String>,
    pub original_name: Option<String>,
    pub mime_type: Option<String>,
    pub is_sticker: bool,
    pub transcription: Option<String>,
    pub sticker_effect: Option<String>,
}

/// Format a Unix second as local / UTC / display strings.
///
/// Returns `None` when the timestamp cannot be represented in local or UTC.
pub fn format_local_ts(secs: i64) -> Option<(String, String, String)> {
    let local = Local.timestamp_opt(secs, 0).single().or_else(|| {
        Utc.timestamp_opt(secs, 0)
            .single()
            .map(|utc| Local.from_utc_datetime(&utc.naive_utc()))
    })?;
    let utc = local.with_timezone(&Utc);
    let display = local.format("%b %e, %Y %I:%M:%S %p").to_string();
    Some((
        local.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        display,
    ))
}

/// Deterministic message GUID from chat + timestamp + direction + body + attachment digests.
pub fn stable_guid(
    chat_id: &str,
    timestamp: &str,
    is_from_me: bool,
    text: &str,
    att_digests: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(chat_id.as_bytes());
    hasher.update(b"|");
    hasher.update(timestamp.as_bytes());
    hasher.update(b"|");
    hasher.update(if is_from_me { b"1" } else { b"0" });
    hasher.update(b"|");
    hasher.update(text.as_bytes());
    for d in att_digests {
        hasher.update(b"|");
        hasher.update(d.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Serialize a value for a CSV JSON cell (`null` on failure).
pub fn json_cell(value: &impl Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

/// Filesystem-safe chat CSV filename (maps non `[A-Za-z0-9_-]` to `_`).
///
/// Does not apply Plus’s empty/`unknown` stem rule — callers that need that
/// should build the stem themselves and append `.csv`.
pub fn safe_filename(chat_id: &str) -> String {
    chat_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        + ".csv"
}
