//! Convert OpenExtract rows → per-conversation vault-shaped CSV.

use crate::parse::{discover_csv_files, parse_csv_file, RawRow, SourceKind};
use anyhow::{Context, Result};
use chrono::DateTime;
use message_contacts::ContactsBook;
use message_csv::{
    format_local_ts, json_cell, safe_filename, stable_guid, AttachmentCell, DateRange,
};
use message_phone::{sanitize_number, to_e164};
use std::collections::BTreeMap;
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
    "text",
    "attachments_json",
    "export_source",
    "export_tool",
    "export_tool_version",
    "source_kind",
    "contact_name",
    "date_ms",
    "openextract_has_attachments",
];

const EXPORT_SOURCE: &str = "openextract";
const EXPORT_TOOL: &str = "OpenExtract";
const EXPORT_TOOL_VERSION: &str = "0.5.1";

#[derive(Debug, Default)]
pub struct ExportReport {
    pub conversations: u64,
    pub messages: u64,
    pub sent: u64,
    pub received: u64,
    pub skipped_invalid_date: u64,
    pub skipped_out_of_range: u64,
    /// Rows/chats where peer was a name with no VCF phone (name-only chat id).
    pub unresolved_chat_phone: u64,
    pub errors: Vec<String>,
}

#[derive(Debug)]
struct PendingMessage {
    sort_key: f64,
    is_from_me: bool,
    sender_handle: String,
    sender_display_name: String,
    text: String,
    contact_name: String,
    date_ms: String,
    has_attachments: bool,
    source_kind: SourceKind,
}

#[derive(Debug, Default)]
struct PendingConversation {
    messages: Vec<PendingMessage>,
}

/// Convert OpenExtract CSV(s) under `input` using `book` (from VCF/contacts).
pub fn convert_export(
    input: &Path,
    output: &Path,
    book: &ContactsBook,
    date_range: &DateRange,
) -> Result<ExportReport> {
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
    clean_previous_csv(output)?;

    let files = discover_csv_files(input)?;
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    // For per-chat files, infer peer once from all rows in that file.
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

        let per_chat_peer = if rows[0].source_kind == SourceKind::PerChat {
            Some(infer_peer_label(&rows))
        } else {
            None
        };

        for row in rows {
            let peer_label = row
                .conversation
                .as_deref()
                .filter(|s| !s.is_empty() && !is_me(s))
                .map(|s| s.to_string())
                .or_else(|| {
                    if !row.is_from_me && !is_me(&row.sender) {
                        Some(row.sender.clone())
                    } else {
                        per_chat_peer.clone()
                    }
                })
                .unwrap_or_else(|| "unknown".to_string());

            let (chat_id, contact_name, unresolved) = resolve_chat(book, &peer_label);
            if unresolved {
                report.unresolved_chat_phone += 1;
            }

            let Some((secs, date_ms)) = parse_timestamp(&row.date) else {
                report.skipped_invalid_date += 1;
                continue;
            };
            if !date_range.contains_secs(secs) {
                report.skipped_out_of_range += 1;
                continue;
            }

            let is_from_me = resolve_is_from_me(&row);
            let (sender_handle, sender_display_name) =
                resolve_sender(book, &row, is_from_me, &chat_id, &contact_name);

            let convo = conversations.entry(chat_id).or_default();
            convo.messages.push(PendingMessage {
                sort_key: secs as f64,
                is_from_me,
                sender_handle,
                sender_display_name,
                text: row.text,
                contact_name,
                date_ms,
                has_attachments: row.has_attachments,
                source_kind: row.source_kind,
            });
        }
    }

    for (chat_id, mut convo) in conversations {
        write_conversation(output, &chat_id, &mut convo, &mut report)?;
    }

    Ok(report)
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

fn is_me(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("me")
}

fn infer_peer_label(rows: &[RawRow]) -> String {
    let mut phone_peer = None;
    let mut name_peer = None;
    for row in rows {
        if row.is_from_me || is_me(&row.sender) {
            continue;
        }
        if sanitize_number(&row.sender).is_some() {
            phone_peer.get_or_insert_with(|| row.sender.clone());
        } else if name_peer.is_none() {
            name_peer = Some(row.sender.clone());
        }
    }
    phone_peer
        .or(name_peer)
        .unwrap_or_else(|| "unknown".to_string())
}

/// Returns `(chat_identifier, contact_name, unresolved_phone)`.
fn resolve_chat(book: &ContactsBook, peer: &str) -> (String, String, bool) {
    let peer = peer.trim();
    if peer.is_empty() || peer.eq_ignore_ascii_case("unknown") {
        return ("unknown".to_string(), String::new(), true);
    }
    if let Some(digits) = sanitize_number(peer) {
        let e164 = to_e164(&digits);
        let name = book
            .lookup_name_by_phone(&digits)
            .unwrap_or("")
            .to_string();
        return (e164, name, false);
    }
    if let Some(e164) = book.lookup_e164_by_name(peer) {
        return (e164, peer.to_string(), false);
    }
    // Name-only chat id — not fatal; vault may struggle later.
    (name_stem(peer), peer.to_string(), true)
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

fn resolve_is_from_me(row: &RawRow) -> bool {
    if let Some(dir) = row.direction.as_deref() {
        let d = dir.trim().to_ascii_lowercase();
        if d == "sent" || d == "outgoing" {
            return true;
        }
        if d == "received" || d == "incoming" {
            return false;
        }
    }
    row.is_from_me
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
    // Prefer phone on chat_id when it looks like E.164.
    let handle = if chat_id.starts_with('+') || sanitize_number(chat_id).is_some() {
        if chat_id.starts_with('+') {
            chat_id.to_string()
        } else {
            sanitize_number(chat_id)
                .map(|d| to_e164(&d))
                .unwrap_or_default()
        }
    } else if let Some(digits) = sanitize_number(&row.sender) {
        to_e164(&digits)
    } else {
        String::new()
    };

    let display = if !contact_name.is_empty() {
        contact_name.to_string()
    } else if let Some(digits) = sanitize_number(&row.sender) {
        book.lookup_name_by_phone(&digits)
            .unwrap_or("")
            .to_string()
    } else if !is_me(&row.sender) {
        row.sender.clone()
    } else {
        String::new()
    };

    (handle, display)
}

fn parse_timestamp(raw: &str) -> Option<(i64, String)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    // RFC3339 / ISO-8601 with offset (OpenExtract style).
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        let secs = dt.timestamp();
        return Some((secs, (secs * 1000).to_string()));
    }
    // Fallback without fractional seconds.
    if let Ok(dt) = DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%z") {
        let secs = dt.timestamp();
        return Some((secs, (secs * 1000).to_string()));
    }
    None
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
    convo.messages.sort_by(|a, b| {
        a.sort_key
            .partial_cmp(&b.sort_key)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    convo.messages.retain(|m| {
        if format_local_ts(m.sort_key as i64).is_some() {
            true
        } else {
            report.skipped_invalid_date += 1;
            false
        }
    });
    if convo.messages.is_empty() {
        return Ok(());
    }

    let filename = if chat_id == "unknown" || !chat_id.starts_with('+') && sanitize_number(chat_id).is_none() {
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

    let empty_atts: Vec<AttachmentCell> = Vec::new();
    let attachments_json = json_cell(&empty_atts);

    for msg in &convo.messages {
        let secs = msg.sort_key as i64;
        let (ts_local, ts_utc, ts_display) =
            format_local_ts(secs).expect("timestamp validated above");
        let guid = stable_guid(chat_id, &ts_local, msg.is_from_me, &msg.text, &[]);
        let direction = if msg.is_from_me {
            "outgoing"
        } else {
            "incoming"
        };
        wtr.write_record([
            chat_id,
            "individual",
            "",
            guid.as_str(),
            ts_local.as_str(),
            ts_utc.as_str(),
            ts_display.as_str(),
            direction,
            "SMS",
            msg.sender_handle.as_str(),
            msg.sender_display_name.as_str(),
            msg.text.as_str(),
            attachments_json.as_str(),
            EXPORT_SOURCE,
            EXPORT_TOOL,
            EXPORT_TOOL_VERSION,
            msg.source_kind.as_str(),
            msg.contact_name.as_str(),
            msg.date_ms.as_str(),
            if msg.has_attachments { "true" } else { "false" },
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
    use message_contacts::ContactsBook;
    use std::io::Write;
    use std::path::PathBuf;

    fn write(dir: &tempfile::TempDir, name: &str, body: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut f = File::create(&path).unwrap();
        write!(f, "{body}").unwrap();
        path
    }

    #[test]
    fn phone_peer_gets_vcf_name() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "conversation_1.csv",
            "Date,Sender,Text,Is From Me,Has Attachments\n\
2020-01-01T12:00:00+00:00,+15555550122,Hello,False,False\n\
2020-01-01T12:01:00+00:00,me,Hi,True,False\n",
        );
        let vcf = write(
            &dir,
            "contacts.vcf",
            "BEGIN:VCARD\nVERSION:3.0\nN:Example;Sam;;;\nFN:Sam Example\n\
TEL;TYPE=CELL:+1-555-555-0122\nEND:VCARD\n",
        );
        let book = ContactsBook::load_vcf(&vcf).unwrap();
        let out = dir.path().join("out");
        let report = convert_export(dir.path(), &out, &book, &DateRange::default()).unwrap();
        assert_eq!(report.conversations, 1);
        assert_eq!(report.unresolved_chat_phone, 0);
        let csv_path = out.join("_15555550122.csv");
        let body = fs::read_to_string(&csv_path).unwrap();
        assert!(body.contains("Sam Example"));
        assert!(body.contains("openextract"));
    }

    #[test]
    fn name_without_phone_still_writes() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "conversation_2.csv",
            "Date,Sender,Text,Is From Me,Has Attachments\n\
2020-01-01T12:00:00+00:00,Cathy Arp,Hi,False,False\n\
2020-01-01T12:01:00+00:00,me,Hello,True,False\n",
        );
        let vcf = write(
            &dir,
            "contacts.vcf",
            "BEGIN:VCARD\nVERSION:3.0\nN:Other;Person;;;\nFN:Other Person\n\
TEL:+15555550999\nEND:VCARD\n",
        );
        let book = ContactsBook::load_vcf(&vcf).unwrap();
        let out = dir.path().join("out");
        let report = convert_export(dir.path(), &out, &book, &DateRange::default()).unwrap();
        assert!(report.unresolved_chat_phone >= 1);
        assert_eq!(report.conversations, 1);
        let csv_path = out.join("Cathy_Arp.csv");
        assert!(csv_path.is_file(), "missing {}", csv_path.display());
    }

    #[test]
    fn date_range_skips_messages_outside_window() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            "conversation_1.csv",
            "Date,Sender,Text,Is From Me,Has Attachments\n\
2019-12-31T23:00:00+00:00,+15555550122,Old,False,False\n\
2020-01-01T12:00:00+00:00,+15555550122,Keep,False,False\n\
2020-01-02T00:00:00+00:00,+15555550122,New,False,False\n",
        );
        let book = ContactsBook::empty();
        let out = dir.path().join("out");
        let range =
            DateRange::parse_optional_tz(Some("2020-01-01"), Some("2020-01-02"), Some("UTC"))
                .unwrap();
        let report = convert_export(dir.path(), &out, &book, &range).unwrap();
        assert_eq!(report.skipped_out_of_range, 2);
        assert_eq!(report.messages, 1);
        let body = fs::read_to_string(out.join("_15555550122.csv")).unwrap();
        assert!(body.contains("Keep"));
        assert!(!body.contains("Old"));
        assert!(!body.contains("New"));
    }
}
