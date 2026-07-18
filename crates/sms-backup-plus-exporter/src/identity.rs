//! Build fingerprint strings so we can tell if two SMS files are the same text.
//!
//! # Why this exists
//!
//! `dedupe-eml` and `convert` see the same text many times (different folders,
//! different backup dates). An **identity** is a short string built from the
//! message. Same identity → treat as one message and keep a single copy.
//!
//! Example: `export_a/flat_received.eml` and `export_b/Sent/flat_received.eml`
//! both say "Hello from Alice" to `+14075551234` at the same time. They get the
//! same identity, so only one file is kept.
//!
//! # Fingerprints
//!
//! 1. **`content_identity`** — who, when (ms), sent/received, text, attachment
//!    hashes. Used for exact content matching and flat detail.
//! 2. **`message_identity`** — if `X-smssync-id` exists, include that id *plus*
//!    who/when/direction/text. Otherwise same as `content_identity`. Used for
//!    flat↔flat dedupe.
//! 3. **`cover_identity`** — who, when floored to whole seconds, sent/received,
//!    text (no attachment hashes). Used only for archive↔flat coverage so
//!    archive body times (`HH:MM:SS`) still match flat `X-smssync-date` ms.
//!
//! # Why we do not use `X-smssync-id` alone
//!
//! That header is Android's message row number. It can repeat across phones or
//! reinstalls. Real bug: two different texts both had `X-smssync-id: 276`
//! (Alex in one chat vs Sam in another). Using only `276` wrongly merged them.
//! We chose to always attach chat, time, direction, and text when that header
//! is present.
//!
//! # Text and time cleanup
//!
//! We collapse whitespace in the body (`"Hello  \n from"` → `"Hello from"`) so
//! tiny export differences do not create two identities. Flat dedupe keeps
//! millisecond precision; archive coverage floors to seconds.

use crate::phone::to_e164;
use crate::types::{AttachmentBlob, ParsedMessage};
use chrono::{DateTime, Local, TimeZone, Utc};
use sha2::{Digest, Sha256};

/// Who this chat is with, as a stable string (E.164 phone or `chat-…` for groups).
pub(crate) fn chat_id_for(msg: &ParsedMessage) -> String {
    if msg.conversation_type == "group" {
        format!("chat-{}", msg.chat_key)
    } else {
        to_e164(&msg.chat_key)
    }
}

/// Message time as milliseconds since 1970 (for identity strings).
pub(crate) fn timestamp_ms(timestamp_secs: f64) -> i64 {
    (timestamp_secs * 1000.0).round() as i64
}

/// Local wall-clock time for a Unix second, never panicking on out-of-range values.
///
/// Tries local interpretation, then UTC mapped to local, then Unix epoch.
pub(crate) fn local_datetime_from_secs(secs: i64) -> DateTime<Local> {
    Local
        .timestamp_opt(secs, 0)
        .single()
        .or_else(|| {
            Utc.timestamp_opt(secs, 0)
                .single()
                .map(|utc| utc.with_timezone(&Local))
        })
        .unwrap_or_else(|| DateTime::UNIX_EPOCH.with_timezone(&Local))
}

/// Clean the body text before fingerprinting.
///
/// Turns newlines into spaces and squeezes repeated spaces so
/// `"Hello  \n\t from Alice\n"` matches `"Hello from Alice"`.
pub(crate) fn normalized_text(text: &str) -> String {
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    unified.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Fingerprint from chat + time + direction + text + attachment hashes.
pub(crate) fn content_identity(msg: &ParsedMessage) -> String {
    message_identity_from_parts(
        &chat_id_for(msg),
        timestamp_ms(msg.timestamp_secs),
        msg.is_from_me,
        &normalized_text(&msg.text),
        &attachment_digests(&msg.attachments),
    )
}

/// Floor millisecond timestamp to the start of its whole second.
pub(crate) fn floor_ms_to_sec(ms: i64) -> i64 {
    ms.div_euclid(1000) * 1000
}

/// Archive↔flat coverage key: chat + whole-second time + direction + text.
///
/// Ignores attachment digests and sub-second time so archive body timestamps
/// (`YYYY-MM-DD HH:MM:SS`) can match flat `X-smssync-date` millisecond values.
pub(crate) fn cover_identity(msg: &ParsedMessage) -> String {
    format!(
        "{}|{}|{}|{}",
        chat_id_for(msg),
        floor_ms_to_sec(timestamp_ms(msg.timestamp_secs)),
        if msg.is_from_me { "1" } else { "0" },
        normalized_text(&msg.text),
    )
}

/// Fingerprint used when deciding if two flat files are duplicates.
///
/// If `X-smssync-id` is set, the key is
/// `smssync:{id}|{chat}|{time_ms}|{sent_or_recv}|{text}`.
/// Otherwise same as [`content_identity`].
///
/// Does not depend on the source file path — only on the message itself.
pub(crate) fn message_identity(msg: &ParsedMessage) -> String {
    if let Some(id) = msg
        .smssync_id
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return format!(
            "smssync:{}|{}|{}|{}|{}",
            id,
            chat_id_for(msg),
            timestamp_ms(msg.timestamp_secs),
            if msg.is_from_me { "1" } else { "0" },
            normalized_text(&msg.text),
        );
    }
    content_identity(msg)
}

/// Same as content fingerprint, when attachment hashes are already known.
pub(crate) fn message_identity_from_parts(
    chat_id: &str,
    timestamp_ms: i64,
    is_from_me: bool,
    text: &str,
    att_digests: &[String],
) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        chat_id,
        timestamp_ms,
        if is_from_me { "1" } else { "0" },
        text,
        att_digests.join(",")
    )
}

pub(crate) fn attachment_digests(blobs: &[AttachmentBlob]) -> Vec<String> {
    blobs
        .iter()
        .map(|b| hex::encode(Sha256::digest(&b.data)))
        .collect()
}

/// Short hash of an identity string for use in output filenames.
pub(crate) fn short_id(identity: &str) -> String {
    let digest = hex::encode(Sha256::digest(identity.as_bytes()));
    digest[..12.min(digest.len())].to_string()
}

/// Make a string safe for a filename (letters, digits, `-`, `_` only).
///
/// Strips leading underscores so E.164 `+1…` does not become `recv__1…`.
pub(crate) fn safe_stem(value: &str) -> String {
    let raw: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = raw.trim_start_matches('_');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_msg(
        smssync_id: Option<&str>,
        chat_key: &str,
        ts: f64,
        is_from_me: bool,
        text: &str,
    ) -> ParsedMessage {
        ParsedMessage {
            chat_key: chat_key.into(),
            conversation_type: "individual".into(),
            group_title: None,
            participant_digits: vec![(chat_key.into(), None)],
            timestamp_secs: ts,
            is_from_me,
            sender_digits: if is_from_me {
                None
            } else {
                Some(chat_key.into())
            },
            text: text.into(),
            attachments: vec![],
            name_hint: None,
            smssync_id: smssync_id.map(str::to_string),
        }
    }

    #[test]
    fn scopes_smssync_id_with_chat_time_and_text() {
        let id = message_identity(&sample_msg(
            Some("276"),
            "4075551234",
            1609459200.0,
            false,
            "Hello",
        ));
        assert_eq!(id, "smssync:276|+14075551234|1609459200000|0|Hello");
    }

    #[test]
    fn same_smssync_id_different_chats_are_distinct() {
        let sam = message_identity(&sample_msg(
            Some("276"),
            "5555550122",
            1609459300.0,
            false,
            "Hello from Sam",
        ));
        let alex = message_identity(&sample_msg(
            Some("276"),
            "5555550111",
            1609459200.313,
            true,
            "Hello from Alex",
        ));
        assert_ne!(sam, alex);
        assert!(sam.starts_with("smssync:276|"));
        assert!(alex.starts_with("smssync:276|"));
        assert!(alex.contains("1609459200313"));
    }

    #[test]
    fn falls_back_to_content_key() {
        let id = message_identity(&sample_msg(
            None,
            "4075551234",
            1609459200.0,
            false,
            "Hello",
        ));
        assert!(id.starts_with("+14075551234|1609459200000|0|Hello|"));
    }

    #[test]
    fn content_identity_collapses_whitespace() {
        let mut spaced = sample_msg(None, "4075551234", 1609459200.5, false, "Hello");
        spaced.text = "Hello  \n\t from\r\nAlice\n".into();
        let compact = sample_msg(None, "4075551234", 1609459200.5, false, "Hello from Alice");
        assert_eq!(content_identity(&spaced), content_identity(&compact));
        assert!(content_identity(&spaced).contains("|1609459200500|"));
    }

    #[test]
    fn normalized_text_collapses_runs() {
        assert_eq!(normalized_text("  a \n\n b\t "), "a b");
    }

    #[test]
    fn cover_identity_floors_ms_and_ignores_attachments() {
        let flat = sample_msg(None, "4075551234", 1609459200.488, false, "Will do");
        let mut archive = sample_msg(None, "4075551234", 1609459200.0, false, "Will do");
        archive.attachments.push(AttachmentBlob {
            filename: "x.jpg".into(),
            original_name: None,
            mime_type: Some("image/jpeg".into()),
            data: b"fake".to_vec(),
        });
        assert_ne!(content_identity(&flat), content_identity(&archive));
        assert_eq!(cover_identity(&flat), cover_identity(&archive));
        assert_eq!(
            cover_identity(&flat),
            "+14075551234|1609459200000|0|Will do"
        );
    }

    #[test]
    fn safe_stem_strips_leading_plus_as_underscore() {
        assert_eq!(safe_stem("+14075551234"), "14075551234");
        assert_eq!(safe_stem("___"), "unknown");
    }
}
