//! Turn a parsed SMS into a flat `.eml` file (one message per file).
//!
//! # What this is for
//!
//! `dedupe-eml` sometimes finds a text only inside an **archive** file (a
//! multi-message dump). Example: `SMS archive Alice` contains
//! `2020-01-01 12:00:00 - Me / Only in archive`, but there is no separate
//! `….eml` for that one text.
//!
//! This module **creates a new** single-message `.eml` from that parsed data
//! so the clean folder can hold it. The file is shaped like SMS Backup+ flats
//! so `convert` can read it later. It is **not** a perfect copy of a lost
//! original backup.
//!
//! # `X-smssync-id` (the sync id header)
//!
//! Real SMS Backup+ flats often have `X-smssync-id` from Android's message
//! database. Archive dumps do not include that number.
//!
//! We chose to **make one up** that stays the same every run:
//! `gen-` plus the first 16 hex digits of a hash of the message content.
//! Example: `gen-a1b2c3d4e5f67890`.
//!
//! Why: our tools only need a stable string in that header. We do not need a
//! real Android id. The `gen-` prefix makes it obvious this was created here.
//!
//! # Which email headers we write
//!
//! An **email header** is a metadata line at the top of an `.eml` (like
//! `From:`, `Date:`, `Subject:`).
//!
//! We write a small set:
//! - `From` / `To`
//! - `Subject` (e.g. `SMS with Alice`)
//! - `Date` and `Message-ID`
//! - `X-smssync-type`, `X-smssync-address`, `X-smssync-date`, `X-smssync-id`
//! - plain text body, or a multipart body if there are attachments
//!
//! We chose **not** to invent Gmail `Received:` chains or other export-tool
//! headers. This project never reads those. Faking them would not help parsing
//! and would look like we had source details we do not have.

use crate::identity::{content_identity, local_datetime_from_secs, timestamp_ms};
use crate::phone::{owner_digits, to_e164};
use crate::types::{AttachmentBlob, ParsedMessage};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use sha2::{Digest, Sha256};

const BOUNDARY: &str = "----=_Part_sms_backup_plus_exporter_gen";

/// Build a made-up but stable `X-smssync-id` (`gen-` + hash of message content).
pub(crate) fn synthetic_smssync_id(msg: &ParsedMessage) -> String {
    let digest = hex::encode(Sha256::digest(content_identity(msg).as_bytes()));
    format!("gen-{}", &digest[..16.min(digest.len())])
}

fn rfc2822_date(secs: f64) -> String {
    local_datetime_from_secs(secs as i64).to_rfc2822()
}

fn address_header(digits_or_unknown: &str) -> String {
    if digits_or_unknown == "Unknown" || digits_or_unknown.is_empty() {
        "unknown@sms-backup-plus.local".into()
    } else {
        let d = to_e164(digits_or_unknown)
            .trim_start_matches('+')
            .to_string();
        format!("{d}@sms-backup-plus.local")
    }
}

fn subject_for(msg: &ParsedMessage) -> String {
    let name = msg
        .name_hint
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if msg.chat_key == "Unknown" {
                "Unknown"
            } else {
                msg.chat_key.as_str()
            }
        });
    format!("SMS with {name}")
}

fn smssync_address(msg: &ParsedMessage) -> String {
    if msg.chat_key == "Unknown" {
        "Unknown".into()
    } else {
        to_e164(&msg.chat_key)
    }
}

fn encode_text_part(text: &str) -> String {
    let body = text.replace('\n', "\r\n");
    format!("Content-Type: text/plain; charset=utf-8\r\n\r\n{body}")
}

fn encode_attachment_part(att: &AttachmentBlob, index: usize) -> String {
    let mime = att
        .mime_type
        .as_deref()
        .unwrap_or("application/octet-stream");
    let name = att.original_name.as_deref().unwrap_or(&att.filename);
    let safe_name = name.replace('"', "'");
    let b64 = B64.encode(&att.data);
    // Wrap long base64 lines at 76 characters (normal for email MIME parts).
    let mut wrapped = String::new();
    for (i, ch) in b64.chars().enumerate() {
        if i > 0 && i % 76 == 0 {
            wrapped.push_str("\r\n");
        }
        wrapped.push(ch);
    }
    format!(
        "Content-Type: {mime}; name=\"{safe_name}\"\r\n\
Content-Transfer-Encoding: base64\r\n\
Content-Disposition: attachment; filename=\"{safe_name}\"\r\n\
X-Attachment-Index: {index}\r\n\
\r\n\
{wrapped}"
    )
}

/// Write `msg` as bytes of a single-message SMS Backup+–compatible `.eml`.
pub(crate) fn write_flat_eml_bytes(msg: &ParsedMessage, owner_phone: &str) -> Vec<u8> {
    let owner = owner_digits(owner_phone);
    let owner_addr = address_header(&owner);
    let peer_addr = address_header(&msg.chat_key);
    let (from, to) = if msg.is_from_me {
        (owner_addr, peer_addr)
    } else {
        (peer_addr, owner_addr)
    };
    let typ = if msg.is_from_me { "2" } else { "1" };
    let sync_id = synthetic_smssync_id(msg);
    let date_ms = timestamp_ms(msg.timestamp_secs);
    let date_hdr = rfc2822_date(msg.timestamp_secs);
    let subject = subject_for(msg);
    let address = smssync_address(msg);
    let msg_id = format!("<{sync_id}@sms-backup-plus-exporter.local>");

    let mut out = String::new();
    out.push_str(&format!("From: {from}\r\n"));
    out.push_str(&format!("To: {to}\r\n"));
    out.push_str(&format!("Subject: {subject}\r\n"));
    out.push_str(&format!("Message-ID: {msg_id}\r\n"));
    out.push_str(&format!("Date: {date_hdr}\r\n"));
    out.push_str("MIME-Version: 1.0\r\n");
    out.push_str(&format!("X-smssync-type: {typ}\r\n"));
    out.push_str(&format!("X-smssync-address: {address}\r\n"));
    out.push_str(&format!("X-smssync-date: {date_ms}\r\n"));
    out.push_str(&format!("X-smssync-id: {sync_id}\r\n"));

    if msg.attachments.is_empty() {
        out.push_str("Content-Type: text/plain; charset=utf-8\r\n\r\n");
        out.push_str(&msg.text.replace('\n', "\r\n"));
        if !msg.text.ends_with('\n') {
            out.push_str("\r\n");
        }
    } else {
        out.push_str(&format!(
            "Content-Type: multipart/mixed; boundary=\"{BOUNDARY}\"\r\n\r\n"
        ));
        out.push_str(&format!("--{BOUNDARY}\r\n"));
        out.push_str(&encode_text_part(&msg.text));
        out.push_str("\r\n");
        for (i, att) in msg.attachments.iter().enumerate() {
            out.push_str(&format!("--{BOUNDARY}\r\n"));
            out.push_str(&encode_attachment_part(att, i + 1));
            out.push_str("\r\n");
        }
        out.push_str(&format!("--{BOUNDARY}--\r\n"));
    }

    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat_eml::parse_flat_eml_mail;

    #[test]
    fn generated_eml_round_trips_through_flat_parser() {
        let msg = ParsedMessage {
            chat_key: "4075551234".into(),
            conversation_type: "individual".into(),
            group_title: None,
            participant_digits: vec![("4075551234".into(), Some("Alice".into()))],
            timestamp_secs: 1609459200.0,
            is_from_me: false,
            sender_digits: Some("4075551234".into()),
            text: "Hello from archive".into(),
            attachments: vec![],
            name_hint: Some("Alice".into()),
            smssync_id: None,
        };
        let bytes = write_flat_eml_bytes(&msg, "5555550100");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gen.eml");
        std::fs::write(&path, &bytes).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let parsed = parse_flat_eml_mail(&path, &mail, "5555550100", &[])
            .unwrap()
            .unwrap();
        assert_eq!(parsed.text.trim(), "Hello from archive");
        assert!(!parsed.is_from_me);
        assert_eq!(parsed.chat_key, "4075551234");
        assert!(parsed.smssync_id.as_ref().unwrap().starts_with("gen-"));
    }
}
