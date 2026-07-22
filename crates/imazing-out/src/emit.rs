//! Convert iMazing Messages / WhatsApp rows → per-conversation vault-shaped CSV.

use crate::parse::{discover_csv_files, parse_csv_file, RawRow, SourceKind};
use anyhow::{Context, Result};
use chrono::{FixedOffset, Local, NaiveDateTime, TimeZone};
use message_contacts::ContactsBook;
use message_csv::{
    format_local_ts, json_cell, parse_utc_offset, safe_filename, stable_guid, AttachmentCell,
    DateRange,
};
use message_phone::{sanitize_number, to_e164};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

/// Windows path components max out at 255 chars; leave room for `__whatsapp.csv` / `.tmp`.
const MAX_FILENAME_STEM: usize = 180;

const HEADERS: &[&str] = &[
    "chat_identifier",
    "conversation_type",
    "group_title",
    "guid",
    "timestamp",
    "timestamp_utc",
    "timestamp_display",
    "direction",
    "service",
    "sender_handle",
    "sender_display_name",
    "subject",
    "text",
    "attachments_json",
    "export_source",
    "export_tool",
    "export_tool_version",
    "contact_name",
    "date_ms",
    "imazing_status",
    "imazing_type",
    "reactions",
    "replying_to",
    "forwarded",
    "attachment_info",
    "delivered_date",
    "read_date",
    "edited_date",
    "deleted_date",
    "sent_date",
];

const EXPORT_SOURCE: &str = "imazing";
const EXPORT_TOOL: &str = "iMazing";
const EXPORT_TOOL_VERSION: &str = "3.5.5";

#[derive(Debug, Default)]
pub struct ExportReport {
    pub conversations: u64,
    pub messages: u64,
    pub sent: u64,
    pub received: u64,
    pub notifications: u64,
    pub skipped_invalid_date: u64,
    pub skipped_out_of_range: u64,
    pub duplicates_dropped: u64,
    /// Chats where peer was a name with no contacts phone (name-only chat id).
    pub unresolved_chat_phone: u64,
    /// Group roster labels that could not be resolved to a phone/email.
    pub unresolved_group_participants: u64,
    pub messages_files: u64,
    pub whatsapp_files: u64,
    pub attachments_saved: u64,
    pub errors: Vec<String>,
}

#[derive(Debug)]
struct PendingMessage {
    sort_key: i64,
    is_from_me: bool,
    is_notification: bool,
    sender_handle: String,
    sender_display_name: String,
    subject: String,
    text: String,
    contact_name: String,
    date_ms: String,
    service: String,
    status: String,
    msg_type: String,
    reactions: String,
    replying_to: String,
    forwarded: String,
    attachment_info: String,
    delivered_date: String,
    read_date: String,
    edited_date: String,
    deleted_date: String,
    sent_date: String,
    attachments: Vec<AttachmentCell>,
}

#[derive(Debug, Default)]
struct PendingConversation {
    conversation_type: String,
    group_title: String,
    source_kind: Option<SourceKind>,
    messages: Vec<PendingMessage>,
    seen: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportFamily {
    Messages,
    WhatsApp,
}

impl TransportFamily {
    fn from_kind(kind: SourceKind) -> Self {
        match kind {
            SourceKind::Messages => Self::Messages,
            SourceKind::WhatsApp => Self::WhatsApp,
        }
    }

    fn key_prefix(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::WhatsApp => "whatsapp",
        }
    }
}

/// Convert iMazing Messages / WhatsApp CSV(s) under `input` using `book` from Contacts CSV.
///
/// `timezone`: fixed UTC offset (e.g. `UTC-05:00`). When `None`, use the host local zone.
/// When `copy_attachments` is true, media files are copied into `output/attachments/`.
pub fn convert_export(
    input: &Path,
    output: &Path,
    book: &ContactsBook,
    timezone: Option<&str>,
    date_range: &DateRange,
    copy_attachments: bool,
) -> Result<ExportReport> {
    let tz = resolve_tz(timezone)?;
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
    clean_previous_csv(output)?;
    let attachments_dir = output.join("attachments");
    if copy_attachments {
        fs::create_dir_all(&attachments_dir)
            .with_context(|| format!("create {}", attachments_dir.display()))?;
    }

    let files = discover_csv_files(input)?;
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    for discovered in &files {
        match discovered.kind {
            SourceKind::Messages => report.messages_files += 1,
            SourceKind::WhatsApp => report.whatsapp_files += 1,
        }
        let rows = match parse_csv_file(&discovered.path, discovered.kind) {
            Ok(r) => r,
            Err(e) => {
                report
                    .errors
                    .push(format!("{}: {e:#}", discovered.path.display()));
                continue;
            }
        };
        if rows.is_empty() {
            continue;
        }

        let family = TransportFamily::from_kind(discovered.kind);
        let mut by_session: BTreeMap<String, Vec<&RawRow>> = BTreeMap::new();
        for row in &rows {
            by_session
                .entry(row.chat_session.clone())
                .or_default()
                .push(row);
        }

        for (session, session_rows) in by_session {
            let peer = collect_peer_info(book, discovered.kind, &session, &session_rows);
            if peer.unresolved_chat {
                report.unresolved_chat_phone += 1;
            }
            report.unresolved_group_participants += peer.unresolved_roster_labels;

            let convo_key = format!("{}|{}", family.key_prefix(), peer.chat_id);
            let convo = conversations.entry(convo_key).or_insert_with(|| PendingConversation {
                conversation_type: if peer.group {
                    "group".into()
                } else {
                    "individual".into()
                },
                group_title: if peer.group {
                    session.clone()
                } else {
                    String::new()
                },
                source_kind: Some(discovered.kind),
                messages: Vec::new(),
                seen: HashSet::new(),
            });

            for row in session_rows {
                let Some((secs, date_ms)) = parse_message_date(&row.message_date, &tz) else {
                    report.skipped_invalid_date += 1;
                    continue;
                };
                if !date_range.contains_secs(secs) {
                    report.skipped_out_of_range += 1;
                    continue;
                }
                let is_notification = is_notification(&row.msg_type);
                let is_from_me = !is_notification && is_outgoing(&row.msg_type);
                let (sender_handle, sender_display_name) =
                    resolve_sender(book, row, is_from_me, is_notification, &peer.chat_id, &peer.contact_name);

                let mut attachments = Vec::new();
                if !row.attachment.is_empty() {
                    let csv_parent = discovered
                        .path
                        .parent()
                        .unwrap_or_else(|| Path::new("."));
                    attachments.push(resolve_attachment_cell(
                        &row.attachment,
                        &row.attachment_type,
                        csv_parent,
                        input,
                        &attachments_dir,
                        copy_attachments,
                        secs,
                        &mut report,
                    ));
                }

                let dedupe_key = format!(
                    "{}|{}|{}|{}|{}",
                    peer.chat_id,
                    secs,
                    if is_from_me { "1" } else { "0" },
                    row.text,
                    row.attachment
                );
                if !convo.seen.insert(dedupe_key) {
                    report.duplicates_dropped += 1;
                    continue;
                }

                let service = if row.service.trim().is_empty() {
                    match discovered.kind {
                        SourceKind::WhatsApp => "WhatsApp".to_string(),
                        SourceKind::Messages => "SMS".to_string(),
                    }
                } else {
                    row.service.clone()
                };

                convo.messages.push(PendingMessage {
                    sort_key: secs,
                    is_from_me,
                    is_notification,
                    sender_handle,
                    sender_display_name,
                    subject: row.subject.clone(),
                    text: row.text.clone(),
                    contact_name: peer.contact_name.clone(),
                    date_ms,
                    service,
                    status: row.status.clone(),
                    msg_type: row.msg_type.clone(),
                    reactions: row.reactions.clone(),
                    replying_to: row.replying_to.clone(),
                    forwarded: row.forwarded.clone(),
                    attachment_info: row.attachment_info.clone(),
                    delivered_date: row.delivered_date.clone(),
                    read_date: row.read_date.clone(),
                    edited_date: row.edited_date.clone(),
                    deleted_date: row.deleted_date.clone(),
                    sent_date: row.sent_date.clone(),
                    attachments,
                });
            }
        }
    }

    for (key, mut convo) in conversations {
        let chat_id = key
            .split_once('|')
            .map(|(_, id)| id.to_string())
            .unwrap_or_else(|| key.clone());
        write_conversation(output, &chat_id, &mut convo, &mut report)?;
    }

    Ok(report)
}

#[derive(Debug)]
struct PeerInfo {
    chat_id: String,
    contact_name: String,
    group: bool,
    unresolved_chat: bool,
    unresolved_roster_labels: u64,
}

fn collect_peer_info(
    book: &ContactsBook,
    kind: SourceKind,
    session: &str,
    rows: &[&RawRow],
) -> PeerInfo {
    let mut handles: HashSet<String> = HashSet::new();
    for row in rows {
        let sid = row.sender_id.trim();
        if is_phone_or_email(sid) {
            if let Some(digits) = sanitize_number(sid) {
                handles.insert(to_e164(&digits));
            } else {
                handles.insert(sid.to_string());
            }
        }
        for phone in phones_in_text(&row.chat_session) {
            handles.insert(phone);
        }
    }

    let mut unresolved_roster_labels = 0u64;
    // Messages group rosters encode members as "A & B & C". Resolve silent members via contacts.
    if kind == SourceKind::Messages && session.contains(" & ") {
        for part in session.split(" & ") {
            let label = part.trim();
            if label.is_empty() {
                continue;
            }
            if let Some(digits) = sanitize_number(label) {
                handles.insert(to_e164(&digits));
                continue;
            }
            if is_phone_or_email(label) {
                handles.insert(label.to_string());
                continue;
            }
            if let Some(e164) = book.lookup_e164_by_name(label) {
                handles.insert(e164);
            } else {
                unresolved_roster_labels += 1;
            }
        }
    }

    let mut peer_handles: Vec<String> = handles.into_iter().collect();
    peer_handles.sort();

    let group = match kind {
        SourceKind::Messages => session.contains(" & ") || peer_handles.len() >= 2,
        // WhatsApp has no roster column; multiple distinct senders imply a group.
        SourceKind::WhatsApp => peer_handles.len() >= 2,
    };

    let (chat_id, contact_name, unresolved_chat) =
        resolve_chat_identifier(book, session, &peer_handles, group);
    PeerInfo {
        chat_id,
        contact_name,
        group,
        unresolved_chat,
        unresolved_roster_labels,
    }
}

#[derive(Debug)]
enum TzMode {
    Local,
    Fixed(FixedOffset),
}

fn resolve_tz(timezone: Option<&str>) -> Result<TzMode> {
    match timezone.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(TzMode::Local),
        Some(name) => {
            let offset = parse_utc_offset(name).map_err(anyhow::Error::msg)?;
            Ok(TzMode::Fixed(offset))
        }
    }
}

fn parse_message_date(raw: &str, tz: &TzMode) -> Option<(i64, String)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let naive = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M"))
        .ok()?;
    let secs = match tz {
        TzMode::Local => Local.from_local_datetime(&naive).single()?.timestamp(),
        TzMode::Fixed(offset) => offset.from_local_datetime(&naive).single()?.timestamp(),
    };
    Some((secs, (secs * 1000).to_string()))
}

fn resolve_attachment_cell(
    csv_name: &str,
    attachment_type: &str,
    csv_parent: &Path,
    input_root: &Path,
    attachments_dir: &Path,
    copy_attachments: bool,
    message_secs: i64,
    report: &mut ExportReport,
) -> AttachmentCell {
    let mime = mime_hint(attachment_type, csv_name);
    let is_sticker = attachment_type.eq_ignore_ascii_case("sticker");
    if !copy_attachments {
        return AttachmentCell {
            path: Some(csv_name.to_string()),
            original_name: Some(csv_name.to_string()),
            mime_type: mime,
            is_sticker,
            transcription: None,
            sticker_effect: None,
        };
    }
    match find_and_copy_attachment(
        csv_name,
        csv_parent,
        input_root,
        attachments_dir,
        message_secs,
        report,
    ) {
        Ok(Some(rel_path)) => AttachmentCell {
            path: Some(rel_path),
            original_name: Some(csv_name.to_string()),
            mime_type: mime,
            is_sticker,
            transcription: None,
            sticker_effect: None,
        },
        Ok(None) | Err(_) => AttachmentCell {
            path: Some(csv_name.to_string()),
            original_name: Some(csv_name.to_string()),
            mime_type: mime,
            is_sticker,
            transcription: None,
            sticker_effect: None,
        },
    }
}

fn attachment_name_matches(disk_name: &str, csv_name: &str) -> bool {
    let disk = disk_name.to_ascii_lowercase();
    let csv = csv_name.to_ascii_lowercase();
    if disk == csv {
        return true;
    }
    disk.ends_with(&csv)
        || disk.ends_with(&format!("_{csv}"))
        || disk.ends_with(&format!("-{csv}"))
}

fn find_attachment_on_disk(csv_name: &str, csv_parent: &Path, input_root: &Path) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(csv_parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && attachment_name_matches(name, csv_name)
            {
                return Some(path);
            }
        }
    }
    find_attachment_walk(csv_name, input_root)
}

fn find_attachment_walk(csv_name: &str, dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_attachment_walk(csv_name, &path) {
                return Some(found);
            }
            continue;
        }
        if !path.is_file() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && attachment_name_matches(name, csv_name)
        {
            return Some(path);
        }
    }
    None
}

fn find_and_copy_attachment(
    csv_name: &str,
    csv_parent: &Path,
    input_root: &Path,
    attachments_dir: &Path,
    message_secs: i64,
    report: &mut ExportReport,
) -> Result<Option<String>> {
    let Some(src) = find_attachment_on_disk(csv_name, csv_parent, input_root) else {
        return Ok(None);
    };
    let bytes = fs::read(&src).with_context(|| format!("read {}", src.display()))?;
    let digest_hex = hex::encode(Sha256::digest(&bytes));
    let digest_prefix = &digest_hex[..16.min(digest_hex.len())];
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let date_prefix = Local
        .timestamp_opt(message_secs, 0)
        .single()
        .map(|t| t.format("%Y%m%d_%H%M%S").to_string())
        .unwrap_or_else(|| message_secs.to_string());
    let name = format!("{date_prefix}-{digest_prefix}{ext}");
    let dest = attachments_dir.join(&name);
    if !dest.exists() {
        fs::write(&dest, &bytes).with_context(|| format!("write {}", dest.display()))?;
        report.attachments_saved += 1;
    }
    Ok(Some(format!("attachments/{name}")))
}

fn clean_previous_csv(output_dir: &Path) -> Result<()> {
    for entry in fs::read_dir(output_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("csv") {
            fs::remove_file(&path)
                .with_context(|| format!("remove previous {}", path.display()))?;
        }
    }
    Ok(())
}

fn is_outgoing(msg_type: &str) -> bool {
    matches!(
        msg_type.trim().to_ascii_lowercase().as_str(),
        "outgoing" | "sent"
    )
}

fn is_notification(msg_type: &str) -> bool {
    msg_type.trim().eq_ignore_ascii_case("notification")
}

fn is_phone_or_email(handle: &str) -> bool {
    if sanitize_number(handle).is_some() {
        return true;
    }
    handle.contains('@') && handle.contains('.')
}

fn phones_in_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i > start + 1 {
                if let Some(digits) = sanitize_number(&text[start..i]) {
                    let e164 = to_e164(&digits);
                    if !out.contains(&e164) {
                        out.push(e164);
                    }
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Returns `(chat_identifier, contact_name, unresolved_phone)`.
fn resolve_chat_identifier(
    book: &ContactsBook,
    session: &str,
    peer_handles: &[String],
    group: bool,
) -> (String, String, bool) {
    if group {
        if !peer_handles.is_empty() {
            let title = session.trim().to_string();
            return (peer_handles.join(","), title, false);
        }
        return (name_stem(session), session.trim().to_string(), true);
    }

    if let Some(handle) = peer_handles.first() {
        let contact_name = if let Some(digits) = sanitize_number(handle) {
            book.lookup_name_by_phone(&digits)
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };
        let contact_name = if contact_name.is_empty() {
            session.trim().to_string()
        } else {
            contact_name
        };
        return (handle.clone(), contact_name, false);
    }

    let session = session.trim();
    if session.is_empty() {
        return ("unknown".to_string(), String::new(), true);
    }
    if let Some(digits) = sanitize_number(session) {
        let e164 = to_e164(&digits);
        let name = book
            .lookup_name_by_phone(&digits)
            .unwrap_or("")
            .to_string();
        return (e164, name, false);
    }
    if let Some(e164) = book.lookup_e164_by_name(session) {
        return (e164, session.to_string(), false);
    }
    (name_stem(session), session.to_string(), true)
}

fn resolve_sender(
    book: &ContactsBook,
    row: &RawRow,
    is_from_me: bool,
    is_notification: bool,
    chat_id: &str,
    contact_name: &str,
) -> (String, String) {
    if is_from_me {
        return (String::new(), String::new());
    }
    if is_notification {
        // Keep any available identity from the notification row; often empty.
        let handle = if let Some(digits) = sanitize_number(&row.sender_id) {
            to_e164(&digits)
        } else if is_phone_or_email(&row.sender_id) {
            row.sender_id.trim().to_string()
        } else {
            String::new()
        };
        return (handle, row.sender_name.trim().to_string());
    }

    let mut handle = String::new();
    if let Some(digits) = sanitize_number(&row.sender_id) {
        handle = to_e164(&digits);
    } else if is_phone_or_email(&row.sender_id) {
        handle = row.sender_id.trim().to_string();
    } else if chat_id.starts_with('+') || sanitize_number(chat_id).is_some() {
        handle = if chat_id.starts_with('+') {
            chat_id.to_string()
        } else {
            sanitize_number(chat_id)
                .map(|d| to_e164(&d))
                .unwrap_or_default()
        };
    } else if !row.sender_name.is_empty() {
        if let Some(e164) = book.lookup_e164_by_name(&row.sender_name) {
            handle = e164;
        }
    }

    let mut display = row.sender_name.trim().to_string();
    if display.is_empty() {
        if let Some(digits) = sanitize_number(&handle) {
            display = book
                .lookup_name_by_phone(&digits)
                .unwrap_or("")
                .to_string();
        }
    }
    if display.is_empty() && !contact_name.is_empty() {
        display = contact_name.to_string();
    }

    (handle, display)
}

fn name_stem(value: &str) -> String {
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
    if raw.is_empty() || raw.chars().all(|c| c == '_') {
        "unknown".to_string()
    } else {
        raw
    }
}

fn conversation_filename(chat_id: &str, source_kind: Option<SourceKind>) -> String {
    let mut stem = if chat_id == "unknown"
        || (!chat_id.starts_with('+')
            && !chat_id.contains(',')
            && !chat_id.contains('@')
            && sanitize_number(chat_id).is_none())
    {
        name_stem(chat_id)
    } else if chat_id.contains(',') {
        name_stem(chat_id)
    } else {
        // safe_filename appends .csv; strip it to work with optional WhatsApp suffix.
        let with_ext = safe_filename(chat_id);
        with_ext
            .strip_suffix(".csv")
            .unwrap_or(&with_ext)
            .to_string()
    };
    if stem.len() > MAX_FILENAME_STEM {
        let mut hasher = Sha256::new();
        hasher.update(chat_id.as_bytes());
        let digest = hex::encode(hasher.finalize());
        stem = format!("group_{}", &digest[..16]);
    }
    if source_kind == Some(SourceKind::WhatsApp) {
        format!("{stem}__whatsapp.csv")
    } else {
        format!("{stem}.csv")
    }
}

fn mime_hint(attachment_type: &str, filename: &str) -> Option<String> {
    let t = attachment_type.trim().to_ascii_lowercase();
    if !t.is_empty() {
        return Some(match t.as_str() {
            "image" => "image/jpeg".into(),
            "video" => "video/mp4".into(),
            "audio" => "audio/mpeg".into(),
            "gif" => "image/gif".into(),
            "sticker" => "image/webp".into(),
            other => other.to_string(),
        });
    }
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".png") {
        Some("image/png".into())
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("image/jpeg".into())
    } else if lower.ends_with(".gif") {
        Some("image/gif".into())
    } else if lower.ends_with(".heic") {
        Some("image/heic".into())
    } else if lower.ends_with(".mp4") || lower.ends_with(".mov") {
        Some("video/mp4".into())
    } else {
        None
    }
}

fn write_conversation(
    output_dir: &Path,
    chat_id: &str,
    convo: &mut PendingConversation,
    report: &mut ExportReport,
) -> Result<()> {
    if convo.messages.is_empty() {
        return Ok(());
    }
    convo.messages.sort_by_key(|m| m.sort_key);
    convo.messages.retain(|m| {
        if format_local_ts(m.sort_key).is_some() {
            true
        } else {
            report.skipped_invalid_date += 1;
            false
        }
    });
    if convo.messages.is_empty() {
        return Ok(());
    }

    let filename = conversation_filename(chat_id, convo.source_kind);

    let path = output_dir.join(filename);
    let mut tmp_name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_else(|| "chat.csv".into());
    tmp_name.push(".tmp");
    let tmp_path = path.with_file_name(tmp_name);
    let file = File::create(&tmp_path).with_context(|| format!("create {}", tmp_path.display()))?;
    let mut wtr = csv::Writer::from_writer(file);
    wtr.write_record(HEADERS)
        .with_context(|| format!("write header {}", path.display()))?;

    for msg in &convo.messages {
        let (ts_local, ts_utc, ts_display) =
            format_local_ts(msg.sort_key).expect("timestamp validated above");
        let digests: Vec<String> = msg
            .attachments
            .iter()
            .filter_map(|a| a.path.clone())
            .collect();
        let guid = stable_guid(
            chat_id,
            &ts_local,
            msg.is_from_me,
            &msg.text,
            &digests,
        );
        let direction = if msg.is_notification {
            "incoming"
        } else if msg.is_from_me {
            "outgoing"
        } else {
            "incoming"
        };
        let attachments_json = json_cell(&msg.attachments);
        wtr.write_record([
            chat_id,
            convo.conversation_type.as_str(),
            convo.group_title.as_str(),
            guid.as_str(),
            ts_local.as_str(),
            ts_utc.as_str(),
            ts_display.as_str(),
            direction,
            msg.service.as_str(),
            msg.sender_handle.as_str(),
            msg.sender_display_name.as_str(),
            msg.subject.as_str(),
            msg.text.as_str(),
            attachments_json.as_str(),
            EXPORT_SOURCE,
            EXPORT_TOOL,
            EXPORT_TOOL_VERSION,
            msg.contact_name.as_str(),
            msg.date_ms.as_str(),
            msg.status.as_str(),
            msg.msg_type.as_str(),
            msg.reactions.as_str(),
            msg.replying_to.as_str(),
            msg.forwarded.as_str(),
            msg.attachment_info.as_str(),
            msg.delivered_date.as_str(),
            msg.read_date.as_str(),
            msg.edited_date.as_str(),
            msg.deleted_date.as_str(),
            msg.sent_date.as_str(),
        ])
        .with_context(|| format!("write row {}", path.display()))?;
        if msg.is_notification {
            report.notifications += 1;
        } else if msg.is_from_me {
            report.sent += 1;
        } else {
            report.received += 1;
        }
        report.messages += 1;
    }

    wtr.flush()?;
    drop(wtr);
    fs::rename(&tmp_path, &path)
        .with_context(|| format!("rename {} → {}", tmp_path.display(), path.display()))?;
    report.conversations += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn write(dir: &tempfile::TempDir, name: &str, body: &str) -> PathBuf {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = File::create(&path).unwrap();
        write!(f, "{body}").unwrap();
        path
    }

    #[test]
    fn name_session_resolves_via_contacts() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages - Bob.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Bob McRoy,2020-01-01 12:00:00,SMS,Incoming,+13212462167,Bob McRoy,Read,,,Hello,,,\n\
Bob McRoy,2020-01-01 12:01:00,SMS,Outgoing,,,Read,,,Hi,,,\n",
        );
        let contacts = write(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Bob,,McRoy,+13212462167,\n",
        );
        let book = ContactsBook::load_imazing_contacts_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let report = convert_export(dir.path(), &out, &book, Some("UTC"), &DateRange::default(), false).unwrap();
        assert_eq!(report.conversations, 1);
        assert_eq!(report.unresolved_chat_phone, 0);
        assert_eq!(report.messages, 2);
        let csv_path = out.join("_13212462167.csv");
        let body = fs::read_to_string(&csv_path).unwrap();
        assert!(body.contains("Bob McRoy"));
        assert!(body.contains("imazing"));
        assert!(body.contains("iMazing"));
        assert!(body.contains("imazing_type"));
    }

    #[test]
    fn name_without_phone_still_writes() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages - Mystery.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Mystery Person,2020-01-01 12:00:00,SMS,Incoming,,,Read,,,Hello,,,\n\
Mystery Person,2020-01-01 12:01:00,SMS,Outgoing,,,Read,,,Hi,,,\n",
        );
        let contacts = write(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Other,,Person,+15555550999,\n",
        );
        let book = ContactsBook::load_imazing_contacts_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let report = convert_export(dir.path(), &out, &book, Some("UTC"), &DateRange::default(), false).unwrap();
        assert!(report.unresolved_chat_phone >= 1);
        assert_eq!(report.conversations, 1);
        assert!(out.join("Mystery_Person.csv").is_file());
    }

    #[test]
    fn drops_exact_duplicate_rows() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Bob,2020-01-01 12:00:00,SMS,Outgoing,,,Read,,,Same,,,\n\
Bob,2020-01-01 12:00:00,SMS,Outgoing,,,Read,,,Same,,,\n",
        );
        let contacts = write(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Bob,,,+15555550100,\n",
        );
        let book = ContactsBook::load_imazing_contacts_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let report = convert_export(dir.path(), &out, &book, Some("UTC"), &DateRange::default(), false).unwrap();
        assert_eq!(report.messages, 1);
        assert_eq!(report.duplicates_dropped, 1);
    }

    #[test]
    fn keeps_same_text_different_attachment() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Bob,2020-01-01 12:00:00,SMS,Incoming,+15555550100,Bob,Read,,,Photo,,a.jpg,Image\n\
Bob,2020-01-01 12:00:00,SMS,Incoming,+15555550100,Bob,Read,,,Photo,,b.jpg,Image\n",
        );
        let contacts = write(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Bob,,,+15555550100,\n",
        );
        let book = ContactsBook::load_imazing_contacts_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let report = convert_export(dir.path(), &out, &book, Some("UTC"), &DateRange::default(), false).unwrap();
        assert_eq!(report.messages, 2);
        assert_eq!(report.duplicates_dropped, 0);
    }

    #[test]
    fn silent_group_member_resolved_via_contacts() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Alice Example & Bob Example & Carol Silent,2020-01-01 12:00:00,iMessage,Incoming,+15555550111,Alice Example,Read,,,Hi,,,\n\
Alice Example & Bob Example & Carol Silent,2020-01-01 12:01:00,iMessage,Incoming,+15555550122,Bob Example,Read,,,Hey,,,\n",
        );
        let contacts = write(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Alice,,Example,+15555550111,\n\
Bob,,Example,+15555550122,\n\
Carol,,Silent,+15555550133,\n",
        );
        let book = ContactsBook::load_imazing_contacts_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let report = convert_export(dir.path(), &out, &book, Some("UTC"), &DateRange::default(), false).unwrap();
        assert_eq!(report.conversations, 1);
        assert_eq!(report.unresolved_group_participants, 0);
        let body = fs::read_to_string(out.join("_15555550111__15555550122__15555550133.csv")).unwrap();
        assert!(body.contains("+15555550133") || body.contains("15555550133"));
        assert!(body.contains("group"));
    }

    #[test]
    fn silent_group_member_without_contacts_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages.csv",
            "Chat Session,Message Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Alice Example & Bob Example & Carol Silent,2020-01-01 12:00:00,iMessage,Incoming,+15555550111,Alice Example,Read,,,Hi,,,\n\
Alice Example & Bob Example & Carol Silent,2020-01-01 12:01:00,iMessage,Incoming,+15555550122,Bob Example,Read,,,Hey,,,\n",
        );
        let contacts = write(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Alice,,Example,+15555550111,\n\
Bob,,Example,+15555550122,\n",
        );
        let book = ContactsBook::load_imazing_contacts_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let report = convert_export(dir.path(), &out, &book, Some("UTC"), &DateRange::default(), false).unwrap();
        assert_eq!(report.conversations, 1);
        assert_eq!(report.unresolved_group_participants, 1);
    }

    #[test]
    fn whatsapp_and_messages_same_peer_stay_separate() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "Messages/chat/Messages - Bob.csv",
            "Chat Session,Message Date,Delivered Date,Read Date,Edited Date,Deleted Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Bob,2020-01-01 12:00:00,,,,,SMS,Incoming,+15555550100,Bob,Read,,,SMS hi,,,\n",
        );
        write(
            &dir,
            "WhatsApp/chat/WhatsApp - Bob.csv",
            "Chat Session,Message Date,Sent Date,Type,Sender ID,Sender Name,Status,Forwarded,Replying to,Text,Reactions,Attachment,Attachment type,Attachment info\n\
Bob,2020-01-01 12:05:00,,Incoming,+15555550100,Bob,Read,,,WA hi,,,,\n",
        );
        let contacts = write(
            &dir,
            "Contacts/All/Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Notes\n\
Bob,,,+15555550100,\n",
        );
        let book = ContactsBook::load_imazing_contacts_csv(&contacts).unwrap();
        let out = dir.path().join("out");
        let report = convert_export(dir.path(), &out, &book, Some("UTC"), &DateRange::default(), false).unwrap();
        assert_eq!(report.conversations, 2);
        assert_eq!(report.messages_files, 1);
        assert_eq!(report.whatsapp_files, 1);
        assert!(out.join("_15555550100.csv").is_file());
        assert!(out.join("_15555550100__whatsapp.csv").is_file());
        let wa = fs::read_to_string(out.join("_15555550100__whatsapp.csv")).unwrap();
        assert!(wa.contains("WhatsApp"));
    }

    #[test]
    fn rejects_unknown_timezone() {
        let err = resolve_tz(Some("America/New_York")).unwrap_err();
        assert!(err.to_string().contains("UTC"));
    }

    #[test]
    fn copies_attachment_by_suffix_match() {
        let dir = tempfile::tempdir().unwrap();
        let chat = dir.path().join("chat");
        fs::create_dir_all(&chat).unwrap();
        let csv = chat.join("Messages - Bob.csv");
        fs::write(
            &csv,
            "Chat Session,Message Date,Delivered Date,Read Date,Edited Date,Deleted Date,Service,Type,Sender ID,Sender Name,Status,Replying to,Subject,Text,Reactions,Attachment,Attachment type\n\
Bob McRoy,2020-01-01 12:00:00,,,,,SMS,Incoming,+15555550100,Bob,Read,,,Hi,,image000000.jpg,Image\n",
        )
        .unwrap();
        fs::write(chat.join("ABC123_image000000.jpg"), b"fake-jpeg-bytes").unwrap();
        let book = ContactsBook::empty();
        let out = dir.path().join("out");
        let report = convert_export(&chat, &out, &book, Some("UTC"), &DateRange::default(), true)
            .unwrap();
        assert_eq!(report.attachments_saved, 1);
        assert_eq!(report.messages, 1);
        let att_dir = out.join("attachments");
        assert!(att_dir.is_dir());
        let count = fs::read_dir(&att_dir).unwrap().count();
        assert_eq!(count, 1);
        let body = fs::read_to_string(out.join("_15555550100.csv")).unwrap();
        assert!(body.contains("attachments/"));
    }
}
