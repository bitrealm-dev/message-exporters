//! Build fingerprint strings so duplicate EML messages collapse to one CSV row.
//!
//! Convert dedupe uses [`cover_identity`]: chat + whole-second time + direction +
//! normalized text. That matches archive body times (`HH:MM:SS`) to flat
//! `X-smssync-date` values that include milliseconds, and ignores `X-smssync-id`
//! so archive↔flat copies of the same SMS collapse.
//!
//! Text normalization collapses whitespace so tiny export differences do not
//! create two identities.

use crate::phone::to_e164;
use crate::types::ParsedMessage;
use chrono::{DateTime, Local, TimeZone, Utc};

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

/// Floor millisecond timestamp to the start of its whole second.
pub(crate) fn floor_ms_to_sec(ms: i64) -> i64 {
    ms.div_euclid(1000) * 1000
}

/// Convert dedupe key: chat + whole-second time + direction + text.
///
/// Ignores attachment digests, sub-second time, and `X-smssync-id` so archive
/// body timestamps match flat `X-smssync-date` ms values.
pub(crate) fn cover_identity(msg: &ParsedMessage) -> String {
    cover_identity_from_parts(
        &chat_id_for(msg),
        timestamp_ms(msg.timestamp_secs),
        msg.is_from_me,
        &normalized_text(&msg.text),
    )
}

pub(crate) fn cover_identity_from_parts(
    chat_id: &str,
    timestamp_ms: i64,
    is_from_me: bool,
    text: &str,
) -> String {
    format!(
        "{}|{}|{}|{}",
        chat_id,
        floor_ms_to_sec(timestamp_ms),
        if is_from_me { "1" } else { "0" },
        text,
    )
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
            smssync_id: None,
            source_kind: "flat".into(),
            android_type: String::new(),
            eml_path: String::new(),
        }
    }

    #[test]
    fn cover_identity_floors_to_second() {
        let whole = sample_msg("4075551234", 1609459200.0, false, "Hello");
        let subsec = sample_msg("4075551234", 1609459200.488, false, "Hello");
        assert_eq!(cover_identity(&whole), cover_identity(&subsec));
        assert_eq!(
            cover_identity(&whole),
            "+14075551234|1609459200000|0|Hello"
        );
    }

    #[test]
    fn cover_identity_ignores_smssync_id() {
        let mut a = sample_msg("4075551234", 1609459200.1, false, "Hello");
        a.smssync_id = Some("1".into());
        let mut b = sample_msg("4075551234", 1609459200.9, false, "Hello");
        b.smssync_id = Some("2".into());
        assert_eq!(cover_identity(&a), cover_identity(&b));
    }

    #[test]
    fn cover_identity_distinct_chats() {
        let a = sample_msg("5555550122", 1609459300.0, false, "Hello from Sam");
        let b = sample_msg("5555550111", 1609459200.313, true, "Hello from Alex");
        assert_ne!(cover_identity(&a), cover_identity(&b));
    }

    #[test]
    fn cover_identity_collapses_whitespace() {
        let mut spaced = sample_msg("4075551234", 1609459200.5, false, "Hello");
        spaced.text = "Hello  \n\t from\r\nAlice\n".into();
        let compact = sample_msg("4075551234", 1609459200.5, false, "Hello from Alice");
        assert_eq!(cover_identity(&spaced), cover_identity(&compact));
    }

    #[test]
    fn normalized_text_collapses_runs() {
        assert_eq!(normalized_text("  a \n\n b\t "), "a b");
    }

    #[test]
    fn safe_stem_strips_leading_plus_as_underscore() {
        assert_eq!(safe_stem("+14075551234"), "14075551234");
        assert_eq!(safe_stem("___"), "unknown");
    }
}
