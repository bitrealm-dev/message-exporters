//! Convert SMS Backup & Restore XML → per-conversation CSV.

use crate::owner_set::OwnerPhoneSet;
use crate::phone::to_e164;
use crate::xml::{parse_xml_file, AttachmentBlob, ConvType, ParsedMessage};
use anyhow::{bail, Context, Result};
use chrono::{Local, TimeZone, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

/// Columns this exporter fills. Shared names match imessage-csv where the
/// concept exists; unused iMessage-only columns are omitted.
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
    // SBR-only
    "export_source",
    "message_kind",
    "date_ms",
    "contact_name",
    "android_type",
    "xml_fields_json",
];

const EXPORT_SOURCE: &str = "sms-backup-restore";

#[derive(Debug, Default)]
pub struct ExportReport {
    pub conversations: u64,
    pub sms_count: u64,
    pub mms_count: u64,
    pub attachments_saved: u64,
    pub sent: u64,
    pub received: u64,
    pub skipped_invalid_date: u64,
    pub skipped_unknown_address: u64,
    pub skipped_unknown_type: u64,
    pub skipped_empty_participants: u64,
    pub skipped_bad_attachment: u64,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
struct PendingAttachment {
    rel_path: String,
    original_name: Option<String>,
    mime_type: Option<String>,
    digest_hex: String,
}

#[derive(Debug, Clone)]
struct PendingMessage {
    sort_key: f64,
    is_from_me: bool,
    sender_digits: Option<String>,
    sender_display_name: Option<String>,
    text: String,
    subject: String,
    attachments: Vec<PendingAttachment>,
    dedupe_key: String,
    message_kind: &'static str,
    date_ms: String,
    contact_name: String,
    android_type: String,
    xml_fields_json: String,
}

#[derive(Debug, Default)]
struct PendingConversation {
    conversation_type: ConvType,
    group_title: Option<String>,
    messages: Vec<PendingMessage>,
}

#[derive(Debug, Serialize)]
struct AttachmentCell {
    path: Option<String>,
    original_name: Option<String>,
    mime_type: Option<String>,
    is_sticker: bool,
    transcription: Option<String>,
    sticker_effect: Option<String>,
}

fn format_local_ts(secs: i64) -> Option<(String, String, String)> {
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

fn stable_guid(
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

fn json_cell(value: &impl Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn chat_id_for(msg: &ParsedMessage) -> String {
    match msg.conversation_type {
        ConvType::Group => format!("chat-{}", msg.chat_key),
        ConvType::Individual => to_e164(&msg.chat_key),
    }
}

fn safe_filename(chat_id: &str) -> String {
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

fn write_attachments(
    blobs: &[AttachmentBlob],
    attachments_dir: &Path,
    report: &mut ExportReport,
) -> Result<Vec<PendingAttachment>> {
    let mut out = Vec::with_capacity(blobs.len());
    for blob in blobs {
        let path = attachments_dir.join(&blob.filename);
        if !path.exists() {
            fs::write(&path, blob.data.as_ref())?;
            report.attachments_saved += 1;
        }
        out.push(PendingAttachment {
            rel_path: format!("attachments/{}", blob.filename),
            original_name: blob.original_name.clone(),
            mime_type: blob.mime_type.clone(),
            digest_hex: blob.digest_hex.clone(),
        });
    }
    Ok(out)
}

fn ensure_convo<'a>(
    map: &'a mut BTreeMap<String, PendingConversation>,
    chat_id: &str,
    conversation_type: ConvType,
    group_title: Option<String>,
) -> &'a mut PendingConversation {
    map.entry(chat_id.to_string())
        .or_insert_with(|| PendingConversation {
            conversation_type,
            group_title,
            messages: Vec::new(),
        })
}

fn add_message(
    conversations: &mut BTreeMap<String, PendingConversation>,
    msg: ParsedMessage,
    pending_atts: Vec<PendingAttachment>,
    report: &mut ExportReport,
) {
    let chat_id = chat_id_for(&msg);
    let convo = ensure_convo(
        conversations,
        &chat_id,
        msg.conversation_type,
        msg.group_title.clone(),
    );
    if msg.is_from_me {
        report.sent += 1;
    } else {
        report.received += 1;
    }
    let att_names: Vec<_> = pending_atts.iter().map(|a| a.rel_path.clone()).collect();
    let dedupe_key = format!(
        "{}|{}|{}|{}",
        msg.timestamp_secs as i64,
        if msg.is_from_me { "1" } else { "0" },
        msg.text,
        att_names.join(",")
    );
    let xml_fields_json = json_cell(&msg.xml_fields);
    convo.messages.push(PendingMessage {
        sort_key: msg.timestamp_secs,
        is_from_me: msg.is_from_me,
        sender_digits: msg.sender_digits,
        sender_display_name: msg.sender_display_name,
        text: msg.text,
        subject: msg.subject,
        attachments: pending_atts,
        dedupe_key,
        message_kind: msg.message_kind,
        date_ms: msg.date_ms,
        contact_name: msg.contact_name,
        android_type: msg.android_type,
        xml_fields_json,
    });
}

fn dedupe_messages(messages: &mut Vec<PendingMessage>) {
    messages.sort_by(|a, b| {
        a.sort_key
            .partial_cmp(&b.sort_key)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut seen = HashSet::new();
    messages.retain(|m| seen.insert(m.dedupe_key.clone()));
}

fn write_conversation(
    output_dir: &Path,
    chat_id: &str,
    convo: &mut PendingConversation,
    report: &mut ExportReport,
) -> Result<()> {
    dedupe_messages(&mut convo.messages);
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

    let path = output_dir.join(safe_filename(chat_id));
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
        let secs = msg.sort_key as i64;
        let (ts_local, ts_utc, ts_display) =
            format_local_ts(secs).expect("timestamp validated above");
        let digests: Vec<String> = msg.attachments.iter().map(|a| a.digest_hex.clone()).collect();
        let guid = stable_guid(chat_id, &ts_local, msg.is_from_me, &msg.text, &digests);
        let direction = if msg.is_from_me {
            "outgoing"
        } else {
            "incoming"
        };
        let (sender_handle, sender_display_name) = if msg.is_from_me {
            (String::new(), String::new())
        } else {
            (
                msg.sender_digits
                    .as_ref()
                    .map(|d| to_e164(d))
                    .unwrap_or_default(),
                msg.sender_display_name.clone().unwrap_or_default(),
            )
        };
        let attachment_cells: Vec<AttachmentCell> = msg
            .attachments
            .iter()
            .map(|a| AttachmentCell {
                path: Some(a.rel_path.clone()),
                original_name: a.original_name.clone(),
                mime_type: a.mime_type.clone(),
                is_sticker: false,
                transcription: None,
                sticker_effect: None,
            })
            .collect();
        let attachments_json = json_cell(&attachment_cells);

        wtr.write_record([
            chat_id,
            convo.conversation_type.as_str(),
            convo.group_title.as_deref().unwrap_or(""),
            guid.as_str(),
            ts_local.as_str(),
            ts_utc.as_str(),
            ts_display.as_str(),
            direction,
            "SMS",
            sender_handle.as_str(),
            sender_display_name.as_str(),
            msg.subject.as_str(),
            msg.text.as_str(),
            attachments_json.as_str(),
            EXPORT_SOURCE,
            msg.message_kind,
            msg.date_ms.as_str(),
            msg.contact_name.as_str(),
            msg.android_type.as_str(),
            msg.xml_fields_json.as_str(),
        ])
        .with_context(|| format!("write row {}", path.display()))?;
    }

    wtr.flush()?;
    drop(wtr);
    fs::rename(&tmp_path, &path)
        .with_context(|| format!("rename {} → {}", tmp_path.display(), path.display()))?;
    Ok(())
}

fn clean_previous_csv(output_dir: &Path) -> Result<()> {
    for entry in fs::read_dir(output_dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.ends_with(".csv") || name.ends_with(".csv.tmp") || name.ends_with(".json") {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
}

fn collect_xml_paths(input: &Path) -> Result<Vec<PathBuf>> {
    if input.is_file() {
        return Ok(vec![input.to_path_buf()]);
    }
    if !input.is_dir() {
        bail!("input is not a file or directory: {}", input.display());
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(input)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
        })
        .collect();
    paths.sort();
    if paths.is_empty() {
        bail!("no .xml files found in {}", input.display());
    }
    Ok(paths)
}

/// Convert SMS Backup & Restore XML into per-conversation CSV.
pub fn convert_export(
    input: &Path,
    output_dir: &Path,
    owner_phones: &[String],
) -> Result<ExportReport> {
    let owners = OwnerPhoneSet::new(owner_phones)?;
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    fs::create_dir_all(output_dir)?;
    clean_previous_csv(output_dir)?;
    let attachments_dir = output_dir.join("attachments");
    fs::create_dir_all(&attachments_dir)?;

    for xml_path in collect_xml_paths(input)? {
        match parse_xml_file(&xml_path, &owners.all_digits) {
            Ok((msgs, stats)) => {
                report.sms_count += stats.sms_count;
                report.mms_count += stats.mms_count;
                report.skipped_invalid_date += stats.skipped_invalid_date;
                report.skipped_unknown_address += stats.skipped_unknown_address;
                report.skipped_unknown_type += stats.skipped_unknown_type;
                report.skipped_empty_participants += stats.skipped_empty_participants;
                report.skipped_bad_attachment += stats.skipped_bad_attachment;
                for msg in msgs {
                    match write_attachments(&msg.attachments, &attachments_dir, &mut report) {
                        Ok(atts) => add_message(&mut conversations, msg, atts, &mut report),
                        Err(err) => report
                            .errors
                            .push(format!("{}: {err:#}", xml_path.display())),
                    }
                }
            }
            Err(err) => report.errors.push(format!("{}: {err:#}", xml_path.display())),
        }
    }

    for (chat_id, mut convo) in conversations {
        write_conversation(output_dir, &chat_id, &mut convo, &mut report)?;
        if !convo.messages.is_empty() {
            report.conversations += 1;
        }
    }

    Ok(report)
}
