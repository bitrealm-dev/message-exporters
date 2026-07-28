//! Per-conversation `.eml` archive writer (SMS/MMS).
//!
//! Layout and headers follow [`docs/MAIL_ARCHIVE.md`](../../../docs/MAIL_ARCHIVE.md).
//! iMessage extensions (tapbacks, balloons, parts, edits) are out of scope here.

use anyhow::{bail, Context, Result};
use chrono::{Local, TimeZone, Utc};
use mail_builder::headers::address::Address;
use mail_builder::headers::date::Date;
use mail_builder::headers::text::Text;
use mail_builder::MessageBuilder;
use message_csv::conversation_filename;
use serde::Serialize;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

const MESSAGE_ID_DOMAIN_DEFAULT: &str = "message-exporters.local";
const MESSAGE_ID_DOMAIN_IMESSAGE: &str = "imessage.local";
const SMS_ADDRESS_DOMAIN: &str = "sms.local";
const HANDLE_ADDRESS_DOMAIN: &str = "handle.local";

/// Message direction for From/To mapping and `X-ME-Direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Incoming,
    Outgoing,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        }
    }
}

/// One participant in a conversation roster.
#[derive(Debug, Clone, Serialize)]
pub struct Participant {
    pub handle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Attachment bytes plus metadata for MIME parts / `X-ME-Attachment-Meta`.
#[derive(Debug, Clone)]
pub struct MailAttachment {
    pub bytes: Vec<u8>,
    pub original_name: Option<String>,
    pub mime_type: Option<String>,
    pub digest_sha256: Option<String>,
    pub is_sticker: bool,
}

/// One SMS/MMS message ready to serialize as a single `.eml`.
#[derive(Debug, Clone)]
pub struct MailMessage {
    pub chat_identifier: String,
    /// `individual` or `group`
    pub conversation_type: String,
    pub group_title: Option<String>,
    pub participants: Vec<Participant>,
    pub guid: String,
    pub timestamp_unix_ms: i64,
    pub direction: Direction,
    pub service: String,
    /// `sms` or `mms`
    pub message_kind: String,
    pub sender_handle: Option<String>,
    pub sender_display_name: Option<String>,
    /// Owner E.164 (or handle) used for From/To mapping.
    pub owner_handle: String,
    pub subject: Option<String>,
    pub text: String,
    pub android_type: Option<String>,
    pub source_fields_json: Option<String>,
    pub export_source: String,
    pub export_tool: String,
    pub export_tool_version: String,
    pub attachments: Vec<MailAttachment>,
}

#[derive(Serialize)]
struct AttachmentMetaCell<'a> {
    path: Option<&'a str>,
    original_name: Option<&'a str>,
    mime_type: Option<&'a str>,
    is_sticker: bool,
    transcription: Option<&'a str>,
    sticker_effect: Option<&'a str>,
    digest_sha256: Option<&'a str>,
}

/// Conversation directory stem (CSV filename without `.csv`).
pub fn conversation_stem(msg: &MailMessage) -> String {
    let participant_handles: Vec<String> = msg
        .participants
        .iter()
        .map(|p| p.handle.clone())
        .collect();
    let csv_name = conversation_filename(
        &msg.conversation_type,
        &msg.chat_identifier,
        msg.group_title.as_deref(),
        &participant_handles,
        None,
    );
    csv_name
        .strip_suffix(".csv")
        .unwrap_or(csv_name.as_str())
        .to_string()
}

/// Write a single `.eml` into an existing conversation directory.
///
/// `sequence` is 1-based (`000001_…`). Creates `conv_dir` if missing.
pub fn write_message_file(
    conv_dir: &Path,
    sequence: u32,
    msg: &MailMessage,
) -> Result<PathBuf> {
    if sequence == 0 {
        bail!("write_message_file sequence must be >= 1");
    }
    fs::create_dir_all(conv_dir)
        .with_context(|| format!("create conversation dir {}", conv_dir.display()))?;
    let secs = msg.timestamp_unix_ms.div_euclid(1000);
    let (date_part, time_part) = local_date_time_parts(secs)
        .with_context(|| format!("invalid timestamp_unix_ms {}", msg.timestamp_unix_ms))?;
    let guid8 = guid_prefix8(&msg.guid);
    let filename = format!("{sequence:06}_{date_part}_{time_part}_{guid8}.eml");
    let path = conv_dir.join(&filename);
    let bytes = build_eml(msg)?;
    let mut file = File::create(&path).with_context(|| format!("create {}", path.display()))?;
    file.write_all(&bytes)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

/// Write one conversation folder of `.eml` files under `output_root`.
///
/// Returns the conversation directory path. Messages are sorted by timestamp,
/// then guid, before emit.
pub fn write_conversation(output_root: &Path, messages: &[MailMessage]) -> Result<PathBuf> {
    if messages.is_empty() {
        bail!("write_conversation requires at least one message");
    }

    let stem = conversation_stem(&messages[0]);
    let conv_dir = output_root.join(&stem);

    let mut ordered: Vec<&MailMessage> = messages.iter().collect();
    ordered.sort_by(|a, b| {
        a.timestamp_unix_ms
            .cmp(&b.timestamp_unix_ms)
            .then_with(|| a.guid.cmp(&b.guid))
    });

    for (idx, msg) in ordered.iter().enumerate() {
        write_message_file(&conv_dir, (idx + 1) as u32, msg)?;
    }

    Ok(conv_dir)
}

fn local_date_time_parts(secs: i64) -> Option<(String, String)> {
    let local = Local.timestamp_opt(secs, 0).single().or_else(|| {
        Utc.timestamp_opt(secs, 0)
            .single()
            .map(|utc| Local.from_utc_datetime(&utc.naive_utc()))
    })?;
    Some((
        local.format("%Y-%m-%d").to_string(),
        local.format("%H%M%S").to_string(),
    ))
}

fn guid_prefix8(guid: &str) -> String {
    let hex: String = guid
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .take(8)
        .collect();
    if hex.len() >= 8 {
        hex[..8].to_string()
    } else if guid.len() >= 8 {
        guid[..8].to_string()
    } else {
        format!("{guid:0<8}")
    }
}

/// Synthetic RFC5322 address for a phone or Apple handle.
///
/// Phones → `+E164@sms.local`. Email / other handles containing `@` →
/// `local=domain@handle.local` (MAIL_ARCHIVE encoding).
pub fn synthetic_address(handle: &str, display_name: Option<&str>) -> Address<'static> {
    let email = if handle.contains('@') {
        let encoded = handle.replace('@', "=");
        format!("{encoded}@{HANDLE_ADDRESS_DOMAIN}")
    } else {
        format!("{handle}@{SMS_ADDRESS_DOMAIN}")
    };
    Address::new_address(display_name.map(|s| s.to_string()), email)
}

fn message_id_domain(msg: &MailMessage) -> &'static str {
    if msg.service.eq_ignore_ascii_case("imessage")
        || msg.message_kind.eq_ignore_ascii_case("imessage")
    {
        MESSAGE_ID_DOMAIN_IMESSAGE
    } else {
        MESSAGE_ID_DOMAIN_DEFAULT
    }
}

fn peer_handle(msg: &MailMessage) -> Option<&str> {
    if msg.conversation_type.eq_ignore_ascii_case("group") {
        return None;
    }
    msg.participants
        .iter()
        .map(|p| p.handle.as_str())
        .find(|h| *h != msg.owner_handle)
        .or_else(|| {
            let id = msg.chat_identifier.as_str();
            if id != msg.owner_handle {
                Some(id)
            } else {
                None
            }
        })
}

fn build_eml(msg: &MailMessage) -> Result<Vec<u8>> {
    let owner_name = None;
    let owner_addr = synthetic_address(&msg.owner_handle, owner_name);

    let (from, to) = match (msg.direction, msg.conversation_type.eq_ignore_ascii_case("group")) {
        (Direction::Incoming, false) => {
            let peer = peer_handle(msg).unwrap_or(msg.chat_identifier.as_str());
            let from = synthetic_address(peer, msg.sender_display_name.as_deref());
            (from, owner_addr)
        }
        (Direction::Outgoing, false) => {
            let peer = peer_handle(msg).unwrap_or(msg.chat_identifier.as_str());
            let peer_name = msg
                .participants
                .iter()
                .find(|p| p.handle == peer)
                .and_then(|p| p.display_name.as_deref());
            (owner_addr, synthetic_address(peer, peer_name))
        }
        (Direction::Incoming, true) => {
            let sender = msg
                .sender_handle
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown");
            let from = synthetic_address(sender, msg.sender_display_name.as_deref());
            let others: Vec<Address<'_>> = msg
                .participants
                .iter()
                .filter(|p| p.handle != sender)
                .map(|p| synthetic_address(&p.handle, p.display_name.as_deref()))
                .chain(std::iter::once(synthetic_address(&msg.owner_handle, None)))
                .collect();
            // Dedup owner if already in participants
            let mut seen = std::collections::HashSet::new();
            let mut uniq = Vec::new();
            for addr in others {
                let email = match &addr {
                    Address::Address(a) => a.email.to_string(),
                    _ => continue,
                };
                if seen.insert(email) {
                    uniq.push(addr);
                }
            }
            (from, Address::new_list(uniq))
        }
        (Direction::Outgoing, true) => {
            let others: Vec<Address<'_>> = msg
                .participants
                .iter()
                .filter(|p| p.handle != msg.owner_handle)
                .map(|p| synthetic_address(&p.handle, p.display_name.as_deref()))
                .collect();
            (owner_addr, Address::new_list(others))
        }
    };

    let subject = mail_subject(msg);
    let date_secs = msg.timestamp_unix_ms.div_euclid(1000);
    let message_id = format!("{}@{}", msg.guid, message_id_domain(msg));

    let mut builder = MessageBuilder::new()
        .from(from)
        .to(to)
        .subject(subject)
        .date(Date::new(date_secs))
        .message_id(message_id)
        .header("X-ME-Chat-Identifier", Text::new(msg.chat_identifier.clone()))
        .header(
            "X-ME-Conversation-Type",
            Text::new(msg.conversation_type.clone()),
        )
        .header("X-ME-Direction", Text::new(msg.direction.as_str()))
        .header("X-ME-Service", Text::new(msg.service.clone()))
        .header("X-ME-Message-Kind", Text::new(msg.message_kind.clone()))
        .header(
            "X-ME-Timestamp-Unix-Ms",
            Text::new(msg.timestamp_unix_ms.to_string()),
        )
        .header("X-ME-Guid", Text::new(msg.guid.clone()))
        .header("X-ME-Export-Source", Text::new(msg.export_source.clone()))
        .header("X-ME-Export-Tool", Text::new(msg.export_tool.clone()))
        .header(
            "X-ME-Export-Tool-Version",
            Text::new(msg.export_tool_version.clone()),
        );

    if let Some(title) = msg.group_title.as_deref().filter(|t| !t.is_empty()) {
        builder = builder.header("X-ME-Group-Title", Text::new(title.to_string()));
    }

    if msg.conversation_type.eq_ignore_ascii_case("group") || !msg.participants.is_empty() {
        let participants_json =
            serde_json::to_string(&msg.participants).unwrap_or_else(|_| "[]".into());
        builder = builder.header("X-ME-Participants", Text::new(participants_json));
    }

    if msg.direction == Direction::Incoming {
        if let Some(h) = msg.sender_handle.as_deref().filter(|s| !s.is_empty()) {
            builder = builder.header("X-ME-Sender-Handle", Text::new(h.to_string()));
        }
        if let Some(n) = msg.sender_display_name.as_deref().filter(|s| !s.is_empty()) {
            builder = builder.header("X-ME-Sender-Display-Name", Text::new(n.to_string()));
        }
    }

    if let Some(subj) = msg.subject.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Subject", Text::new(subj.to_string()));
    }
    if let Some(android) = msg.android_type.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Android-Type", Text::new(android.to_string()));
    }
    if let Some(fields) = msg.source_fields_json.as_deref().filter(|s| !s.is_empty()) {
        builder = builder.header("X-ME-Source-Fields", Text::new(fields.to_string()));
    }

    if !msg.attachments.is_empty() {
        let meta: Vec<AttachmentMetaCell<'_>> = msg
            .attachments
            .iter()
            .map(|a| AttachmentMetaCell {
                path: None,
                original_name: a.original_name.as_deref(),
                mime_type: a.mime_type.as_deref(),
                is_sticker: a.is_sticker,
                transcription: None,
                sticker_effect: None,
                digest_sha256: a.digest_sha256.as_deref(),
            })
            .collect();
        let meta_json = serde_json::to_string(&meta).unwrap_or_else(|_| "[]".into());
        builder = builder.header("X-ME-Attachment-Meta", Text::new(meta_json));
    }

    builder = builder.text_body(msg.text.clone());

    for (i, att) in msg.attachments.iter().enumerate() {
        let mime = att
            .mime_type
            .as_deref()
            .filter(|m| !m.is_empty())
            .unwrap_or("application/octet-stream");
        let filename = att
            .original_name
            .clone()
            .unwrap_or_else(|| format!("attachment-{i}"));
        builder = builder.attachment(mime, filename, att.bytes.clone());
    }

    builder
        .write_to_vec()
        .context("serialize message with mail-builder")
}

fn mail_subject(msg: &MailMessage) -> String {
    if let Some(s) = msg.subject.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        return s.to_string();
    }
    if let Some(t) = msg
        .group_title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        return t.to_string();
    }
    if let Some(n) = msg
        .sender_display_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        return n.to_string();
    }
    if let Some(peer) = peer_handle(msg) {
        return peer.to_string();
    }
    let preview: String = msg.text.chars().take(40).collect();
    if preview.is_empty() {
        msg.chat_identifier.clone()
    } else {
        preview
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mailparse::MailHeaderMap;

    fn base_sms() -> MailMessage {
        MailMessage {
            chat_identifier: "+15555550101".into(),
            conversation_type: "individual".into(),
            group_title: None,
            participants: vec![Participant {
                handle: "+15555550101".into(),
                display_name: Some("Sam".into()),
            }],
            guid: "aabbccddeeff00112233445566778899".into(),
            timestamp_unix_ms: 1_400_773_261_000,
            direction: Direction::Incoming,
            service: "SMS".into(),
            message_kind: "sms".into(),
            sender_handle: Some("+15555550101".into()),
            sender_display_name: Some("Sam".into()),
            owner_handle: "+15555550100".into(),
            subject: None,
            text: "hello from sms".into(),
            android_type: Some("1".into()),
            source_fields_json: Some(r#"{"address":"+15555550101"}"#.into()),
            export_source: "sms-backup-restore".into(),
            export_tool: "SMS Backup & Restore".into(),
            export_tool_version: "10.26.003".into(),
            attachments: vec![],
        }
    }

    #[test]
    fn writes_individual_sms_text_only() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_conversation(tmp.path(), &[base_sms()]).unwrap();
        assert_eq!(dir.file_name().unwrap(), "+15555550101");

        let mut emls: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("eml"))
            .collect();
        emls.sort();
        assert_eq!(emls.len(), 1);
        assert!(emls[0]
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("000001_"));

        let bytes = fs::read(&emls[0]).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = mail.get_headers();
        assert_eq!(
            headers.get_first_value("X-ME-Chat-Identifier").as_deref(),
            Some("+15555550101")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Direction").as_deref(),
            Some("incoming")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Message-Kind").as_deref(),
            Some("sms")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Guid").as_deref(),
            Some("aabbccddeeff00112233445566778899")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Export-Source").as_deref(),
            Some("sms-backup-restore")
        );
        let mid = headers.get_first_value("Message-ID").unwrap();
        assert!(mid.contains("aabbccddeeff00112233445566778899@message-exporters.local"));
        assert!(!headers.get_first_value("In-Reply-To").is_some());
        let body = mail.get_body().unwrap();
        assert!(body.contains("hello from sms"));
        assert!(!mail.ctype.mimetype.starts_with("multipart/"));
    }

    #[test]
    fn writes_group_mms_with_image_part() {
        let mut msg = base_sms();
        msg.chat_identifier = "chat-group1".into();
        msg.conversation_type = "group".into();
        msg.group_title = Some("Family".into());
        msg.message_kind = "mms".into();
        msg.participants = vec![
            Participant {
                handle: "+15555550101".into(),
                display_name: Some("Sam".into()),
            },
            Participant {
                handle: "+15555550102".into(),
                display_name: Some("Alex".into()),
            },
        ];
        msg.attachments = vec![MailAttachment {
            bytes: b"\xff\xd8\xfffakejpeg".to_vec(),
            original_name: Some("photo.jpg".into()),
            mime_type: Some("image/jpeg".into()),
            digest_sha256: Some("deadbeef".into()),
            is_sticker: false,
        }];

        let tmp = tempfile::tempdir().unwrap();
        let dir = write_conversation(tmp.path(), &[msg]).unwrap();
        assert_eq!(dir.file_name().unwrap(), "Family");

        let eml = fs::read_dir(&dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let bytes = fs::read(&eml).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = mail.get_headers();
        assert_eq!(
            headers.get_first_value("X-ME-Conversation-Type").as_deref(),
            Some("group")
        );
        assert_eq!(
            headers.get_first_value("X-ME-Group-Title").as_deref(),
            Some("Family")
        );
        let participants = headers.get_first_value("X-ME-Participants").unwrap();
        assert!(participants.contains("+15555550101"));
        assert!(participants.contains("+15555550102"));
        let meta = headers.get_first_value("X-ME-Attachment-Meta").unwrap();
        assert!(meta.contains("photo.jpg"));
        assert!(meta.contains("deadbeef"));
        assert!(mail.ctype.mimetype.starts_with("multipart/"));

        let mut found_image = false;
        fn walk(m: &mailparse::ParsedMail<'_>, found: &mut bool) {
            if m.ctype.mimetype == "image/jpeg" {
                *found = true;
            }
            for sub in &m.subparts {
                walk(sub, found);
            }
        }
        walk(&mail, &mut found_image);
        assert!(found_image, "expected image/jpeg MIME part");
    }

    #[test]
    fn encodes_email_handles_and_imessage_message_id() {
        let mut msg = base_sms();
        msg.chat_identifier = "friend@icloud.com".into();
        msg.participants = vec![Participant {
            handle: "friend@icloud.com".into(),
            display_name: Some("Friend".into()),
        }];
        msg.sender_handle = Some("friend@icloud.com".into());
        msg.owner_handle = "me@icloud.com".into();
        msg.service = "iMessage".into();
        msg.message_kind = "imessage".into();
        msg.guid = "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE".into();
        msg.export_source = "imessage".into();

        let tmp = tempfile::tempdir().unwrap();
        let path = write_message_file(&tmp.path().join("chat"), 1, &msg).unwrap();
        let bytes = fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let headers = mail.get_headers();
        let from = headers.get_first_value("From").unwrap();
        assert!(
            from.contains("friend=icloud.com@handle.local"),
            "From was {from}"
        );
        let to = headers.get_first_value("To").unwrap();
        assert!(
            to.contains("me=icloud.com@handle.local"),
            "To was {to}"
        );
        let mid = headers.get_first_value("Message-ID").unwrap();
        assert!(
            mid.contains("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE@imessage.local"),
            "Message-ID was {mid}"
        );
    }
}
