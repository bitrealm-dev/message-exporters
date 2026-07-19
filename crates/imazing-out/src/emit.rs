//! Convert iMazing Messages rows → per-conversation vault-shaped CSV.

use crate::parse::{discover_csv_files, parse_csv_file, RawRow};
use anyhow::{Context, Result};
use chrono::{Local, NaiveDateTime, TimeZone};
use chrono_tz::Tz;
use message_contacts::ContactsBook;
use message_csv::{format_local_ts, json_cell, safe_filename, stable_guid, AttachmentCell};
use message_phone::{sanitize_number, to_e164};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::path::Path;

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
    "reactions",
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
    pub skipped_invalid_date: u64,
    pub duplicates_dropped: u64,
    /// Chats where peer was a name with no contacts phone (name-only chat id).
    pub unresolved_chat_phone: u64,
    pub errors: Vec<String>,
}

#[derive(Debug)]
struct PendingMessage {
    sort_key: i64,
    is_from_me: bool,
    sender_handle: String,
    sender_display_name: String,
    subject: String,
    text: String,
    contact_name: String,
    date_ms: String,
    service: String,
    status: String,
    reactions: String,
    attachments: Vec<AttachmentCell>,
}

#[derive(Debug, Default)]
struct PendingConversation {
    conversation_type: String,
    group_title: String,
    messages: Vec<PendingMessage>,
    seen: HashSet<String>,
}

/// Convert iMazing Messages CSV(s) under `input` using `book` from Contacts CSV.
///
/// `timezone`: IANA name (e.g. `America/New_York`). When `None`, use the host local zone.
pub fn convert_export(
    input: &Path,
    output: &Path,
    book: &ContactsBook,
    timezone: Option<&str>,
) -> Result<ExportReport> {
    let tz = resolve_tz(timezone)?;
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
    clean_previous_csv(output)?;

    let files = discover_csv_files(input)?;
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    for path in &files {
        let rows = match parse_csv_file(path) {
            Ok(r) => r,
            Err(e) => {
                report.errors.push(format!("{}: {e:#}", path.display()));
                continue;
            }
        };
        if rows.is_empty() {
            continue;
        }

        let mut by_session: BTreeMap<String, Vec<&RawRow>> = BTreeMap::new();
        for row in &rows {
            by_session
                .entry(row.chat_session.clone())
                .or_default()
                .push(row);
        }

        for (session, session_rows) in by_session {
            let peer_handles = collect_peer_handles(&session_rows);
            let group = is_group(&session, &peer_handles);
            let (chat_id, contact_name, unresolved) =
                resolve_chat_identifier(book, &session, &peer_handles, group);
            if unresolved {
                report.unresolved_chat_phone += 1;
            }

            let convo = conversations.entry(chat_id.clone()).or_insert_with(|| {
                PendingConversation {
                    conversation_type: if group {
                        "group".into()
                    } else {
                        "individual".into()
                    },
                    group_title: if group { session.clone() } else { String::new() },
                    messages: Vec::new(),
                    seen: HashSet::new(),
                }
            });

            for row in session_rows {
                let Some((secs, date_ms)) = parse_message_date(&row.message_date, &tz) else {
                    report.skipped_invalid_date += 1;
                    continue;
                };
                let is_from_me = is_outgoing(&row.msg_type);
                let (sender_handle, sender_display_name) =
                    resolve_sender(book, row, is_from_me, &chat_id, &contact_name);

                let dedupe_key = format!(
                    "{}|{}|{}|{}",
                    chat_id,
                    secs,
                    if is_from_me { "1" } else { "0" },
                    row.text
                );
                if !convo.seen.insert(dedupe_key) {
                    report.duplicates_dropped += 1;
                    continue;
                }

                let mut attachments = Vec::new();
                if !row.attachment.is_empty() {
                    let mime = mime_hint(&row.attachment_type, &row.attachment);
                    attachments.push(AttachmentCell {
                        path: Some(row.attachment.clone()),
                        original_name: Some(row.attachment.clone()),
                        mime_type: mime,
                        is_sticker: false,
                        transcription: None,
                        sticker_effect: None,
                    });
                }

                let service = if row.service.trim().is_empty() {
                    "SMS".to_string()
                } else {
                    row.service.clone()
                };

                convo.messages.push(PendingMessage {
                    sort_key: secs,
                    is_from_me,
                    sender_handle,
                    sender_display_name,
                    subject: row.subject.clone(),
                    text: row.text.clone(),
                    contact_name: contact_name.clone(),
                    date_ms,
                    service,
                    status: row.status.clone(),
                    reactions: row.reactions.clone(),
                    attachments,
                });
            }
        }
    }

    for (chat_id, mut convo) in conversations {
        write_conversation(output, &chat_id, &mut convo, &mut report)?;
    }

    Ok(report)
}

#[derive(Debug)]
enum TzMode {
    Local,
    Named(Tz),
}

fn resolve_tz(timezone: Option<&str>) -> Result<TzMode> {
    match timezone {
        None => Ok(TzMode::Local),
        Some(name) => {
            let tz: Tz = name
                .parse()
                .map_err(|_| anyhow::anyhow!("unknown timezone {name:?} (use IANA, e.g. America/New_York)"))?;
            Ok(TzMode::Named(tz))
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
        TzMode::Named(tz) => tz.from_local_datetime(&naive).single()?.timestamp(),
    };
    Some((secs, (secs * 1000).to_string()))
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

fn collect_peer_handles(rows: &[&RawRow]) -> Vec<String> {
    let mut handles = HashSet::new();
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
    let mut list: Vec<String> = handles.into_iter().collect();
    list.sort();
    list
}

fn is_group(chat_session: &str, peer_handles: &[String]) -> bool {
    chat_session.contains(" & ") || peer_handles.len() >= 2
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
    chat_id: &str,
    contact_name: &str,
) -> (String, String) {
    if is_from_me {
        return (String::new(), String::new());
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

fn mime_hint(attachment_type: &str, filename: &str) -> Option<String> {
    let t = attachment_type.trim().to_ascii_lowercase();
    if !t.is_empty() {
        return Some(match t.as_str() {
            "image" => "image/jpeg".into(),
            "video" => "video/mp4".into(),
            "audio" => "audio/mpeg".into(),
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

    let filename = if chat_id == "unknown"
        || (!chat_id.starts_with('+')
            && !chat_id.contains(',')
            && !chat_id.contains('@')
            && sanitize_number(chat_id).is_none())
    {
        format!("{}.csv", name_stem(chat_id))
    } else if chat_id.contains(',') {
        format!("{}.csv", name_stem(chat_id))
    } else {
        safe_filename(chat_id)
    };

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
        let direction = if msg.is_from_me {
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
            msg.reactions.as_str(),
        ])
        .with_context(|| format!("write row {}", path.display()))?;
        if msg.is_from_me {
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
        let report = convert_export(dir.path(), &out, &book, Some("UTC")).unwrap();
        assert_eq!(report.conversations, 1);
        assert_eq!(report.unresolved_chat_phone, 0);
        assert_eq!(report.messages, 2);
        let csv_path = out.join("_13212462167.csv");
        let body = fs::read_to_string(&csv_path).unwrap();
        assert!(body.contains("Bob McRoy"));
        assert!(body.contains("imazing"));
        assert!(body.contains("iMazing"));
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
        let report = convert_export(dir.path(), &out, &book, Some("UTC")).unwrap();
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
        let report = convert_export(dir.path(), &out, &book, Some("UTC")).unwrap();
        assert_eq!(report.messages, 1);
        assert_eq!(report.duplicates_dropped, 1);
    }

    #[test]
    fn rejects_unknown_timezone() {
        let err = resolve_tz(Some("Not/AZone")).unwrap_err();
        assert!(err.to_string().contains("timezone"));
    }
}
