//! Convert GO SMS Pro export → per-conversation CSV.

use crate::pdu::{parse_pdu_file, ParsedPdu};
use crate::xml::{parse_xml_file, XmlMessage};
use anyhow::{bail, Context, Result};
use chrono::{Local, TimeZone};
use message_csv::{
    format_local_ts, json_cell, safe_filename, stable_guid, AttachmentCell,
};
use message_phone::{to_e164, OwnerPhoneSet};
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
    "text",
    "attachments_json",
    // SMS-Pro-only
    "export_source",
    "source_kind",
    "android_type",
    "date_ms",
    "contact_name",
    "pdu_filename",
    "xml_fields_json",
];

const EXPORT_SOURCE: &str = "go-sms-pro";

#[derive(Debug, Default)]
pub struct ExportReport {
    pub conversations: u64,
    /// XML `<SMS>` rows seen while parsing (before write / dedupe).
    pub xml_messages_seen: u64,
    /// PDU files that produced a pending message (before write / dedupe).
    pub pdu_messages: u64,
    pub pdu_group_messages: u64,
    pub attachments_saved: u64,
    /// Rows written to CSV after dedupe (outgoing).
    pub sent: u64,
    /// Rows written to CSV after dedupe (incoming).
    pub received: u64,
    pub skipped_invalid_date: u64,
    pub skipped_unknown_type: u64,
    pub skipped_unknown_address: u64,
    pub skipped_unparseable_pdu: u64,
    /// PDU parsed but no non-owner participant (self-only / empty PLMN set).
    pub skipped_no_other_party: u64,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
struct PendingAttachment {
    /// Relative path under export dir, e.g. `attachments/20200101_000000-I_…_1.jpg`
    rel_path: String,
    original_name: Option<String>,
    mime_type: Option<String>,
    /// Bytes already written (for guid fingerprint).
    digest_hex: String,
}

#[derive(Debug, Clone)]
struct PendingMessage {
    sort_key: f64,
    is_from_me: bool,
    sender_digits: Option<String>,
    sender_display_name: Option<String>,
    text: String,
    attachments: Vec<PendingAttachment>,
    /// For within-thread dedupe.
    dedupe_key: String,
    source_kind: &'static str,
    android_type: String,
    date_ms: String,
    contact_name: String,
    pdu_filename: String,
    xml_fields: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
struct PendingConversation {
    conversation_type: String,
    group_title: Option<String>,
    messages: Vec<PendingMessage>,
}

fn mime_for_ext(ext: &str) -> Option<&'static str> {
    match ext {
        ".jpg" | ".jpeg" => Some("image/jpeg"),
        ".png" => Some("image/png"),
        ".gif" => Some("image/gif"),
        ".3gp" => Some("video/3gpp"),
        ".mp4" => Some("video/mp4"),
        ".amr" => Some("audio/amr"),
        ".wav" => Some("audio/wav"),
        _ => None,
    }
}

fn chat_id_individual(digits: &str) -> String {
    to_e164(digits)
}

fn chat_id_group(participant_digits: &[String], owners: &OwnerPhoneSet) -> (String, String) {
    let mut others: Vec<String> = participant_digits
        .iter()
        .filter(|d| !d.is_empty() && !owners.is_owner(d))
        .cloned()
        .collect();
    others.sort();
    others.dedup();
    let title = if others.is_empty() {
        "Group".to_string()
    } else if others.len() <= 4 {
        format!(
            "Group: {}",
            others
                .iter()
                .map(|d| to_e164(d))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!(
            "Group: {}, and {} others",
            others[..4]
                .iter()
                .map(|d| to_e164(d))
                .collect::<Vec<_>>()
                .join(", "),
            others.len() - 4
        )
    };
    let slug = others
        .iter()
        .map(|d| d.as_str())
        .collect::<Vec<_>>()
        .join("_");
    let id = if slug.is_empty() {
        "chat-group-unknown".to_string()
    } else {
        format!("chat-group-{slug}")
    };
    // Keep filesystem-safe length.
    let id = if id.len() > 180 {
        let digest = hex::encode(Sha256::digest(id.as_bytes()));
        format!("chat-group-{}", &digest[..16])
    } else {
        id
    };
    (id, title)
}

fn ensure_convo<'a>(
    map: &'a mut BTreeMap<String, PendingConversation>,
    chat_id: &str,
    conversation_type: &str,
    group_title: Option<String>,
) -> &'a mut PendingConversation {
    map.entry(chat_id.to_string())
        .or_insert_with(|| PendingConversation {
            conversation_type: conversation_type.to_string(),
            group_title,
            messages: Vec::new(),
        })
}

fn add_xml_messages(
    conversations: &mut BTreeMap<String, PendingConversation>,
    msgs: Vec<XmlMessage>,
) {
    for msg in msgs {
        let chat_id = chat_id_individual(&msg.other_digits);
        let convo = ensure_convo(conversations, &chat_id, "individual", None);
        let dedupe_key = format!(
            "{}|{}|{}|",
            msg.timestamp_secs as i64,
            if msg.is_from_me { "1" } else { "0" },
            msg.text
        );
        convo.messages.push(PendingMessage {
            sort_key: msg.timestamp_secs,
            is_from_me: msg.is_from_me,
            sender_digits: msg.sender_digits,
            sender_display_name: msg.name_hint.clone(),
            text: msg.text,
            attachments: Vec::new(),
            dedupe_key,
            source_kind: "xml",
            android_type: msg.android_type,
            date_ms: msg.date_ms,
            contact_name: msg.contact_name,
            pdu_filename: String::new(),
            xml_fields: msg.xml_fields,
        });
    }
}

fn save_pdu_attachments(
    parsed: &ParsedPdu,
    attachments_dir: &Path,
    report: &mut ExportReport,
) -> Result<Vec<PendingAttachment>> {
    fs::create_dir_all(attachments_dir)?;
    let date_prefix = Local
        .timestamp_opt(parsed.timestamp, 0)
        .single()
        .map(|t| t.format("%Y%m%d_%H%M%S").to_string())
        .unwrap_or_else(|| parsed.timestamp.to_string());

    let mut out = Vec::new();
    for (idx, att) in parsed.attachments.iter().enumerate() {
        let digest_hex = hex::encode(Sha256::digest(&att.data));
        let digest_prefix = &digest_hex[..16.min(digest_hex.len())];
        let name = format!(
            "{}-I_{}_{}_{}{}",
            date_prefix,
            parsed.timestamp,
            digest_prefix,
            idx + 1,
            att.ext
        );
        let path = attachments_dir.join(&name);
        // Content-addressed name: rewrite only when missing (same bytes → same path).
        if !path.exists() {
            fs::write(&path, &att.data)?;
            report.attachments_saved += 1;
        }
        out.push(PendingAttachment {
            rel_path: format!("attachments/{name}"),
            original_name: att.smil_name.clone().or(Some(name)),
            mime_type: mime_for_ext(&att.ext).map(|s| s.to_string()),
            digest_hex,
        });
    }
    Ok(out)
}

fn add_pdu_message(
    conversations: &mut BTreeMap<String, PendingConversation>,
    parsed: ParsedPdu,
    attachments: Vec<PendingAttachment>,
    owners: &OwnerPhoneSet,
    report: &mut ExportReport,
) {
    let targets: Vec<(String, String, Option<String>)> = if parsed.is_group {
        let (id, title) = chat_id_group(&parsed.participants, owners);
        vec![(id, "group".to_string(), Some(title))]
    } else {
        let others: Vec<_> = parsed
            .participants
            .iter()
            .filter(|p| !p.is_empty() && !owners.is_owner(p))
            .cloned()
            .collect();
        if others.is_empty() {
            report.skipped_no_other_party += 1;
            return;
        }
        let other = &others[0];
        vec![(
            chat_id_individual(other),
            "individual".to_string(),
            None,
        )]
    };

    report.pdu_messages += 1;
    if parsed.is_group {
        report.pdu_group_messages += 1;
    }

    let att_names: Vec<String> = attachments.iter().map(|a| a.rel_path.clone()).collect();
    let dedupe_key = format!(
        "{}|{}|{}|{}",
        parsed.timestamp,
        if parsed.is_sent { "1" } else { "0" },
        parsed.body,
        att_names.join(",")
    );

    let pdu_filename = parsed
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let sender_digits = if parsed.is_sent {
        None
    } else if parsed.sender_number.is_empty() {
        None
    } else {
        Some(parsed.sender_number.clone())
    };

    let pending = PendingMessage {
        sort_key: parsed.timestamp as f64,
        is_from_me: parsed.is_sent,
        sender_digits,
        sender_display_name: None,
        text: parsed.body.clone(),
        attachments,
        dedupe_key,
        source_kind: "pdu",
        android_type: String::new(),
        date_ms: String::new(),
        contact_name: String::new(),
        pdu_filename,
        xml_fields: BTreeMap::new(),
    };

    for (chat_id, conversation_type, group_title) in targets {
        let convo = ensure_convo(conversations, &chat_id, &conversation_type, group_title);
        convo.messages.push(pending.clone());
    }
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
        let xml_fields_json = if msg.xml_fields.is_empty() {
            String::new()
        } else {
            json_cell(&msg.xml_fields)
        };

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
            msg.text.as_str(),
            attachments_json.as_str(),
            EXPORT_SOURCE,
            msg.source_kind,
            msg.android_type.as_str(),
            msg.date_ms.as_str(),
            msg.contact_name.as_str(),
            msg.pdu_filename.as_str(),
            xml_fields_json.as_str(),
        ])
        .with_context(|| format!("write row {}", path.display()))?;

        if msg.is_from_me {
            report.sent += 1;
        } else {
            report.received += 1;
        }
    }

    wtr.flush()?;
    drop(wtr);
    fs::rename(&tmp_path, &path)
        .with_context(|| format!("rename {} → {}", tmp_path.display(), path.display()))?;
    report.conversations += 1;
    Ok(())
}

/// Convert a GO SMS Pro export directory into per-conversation CSV.
pub fn convert_export(
    input_dir: &Path,
    output_dir: &Path,
    owner_phones: &[String],
) -> Result<ExportReport> {
    if !input_dir.is_dir() {
        bail!("input is not a directory: {}", input_dir.display());
    }

    let owners = OwnerPhoneSet::new(owner_phones)?;
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    // Clean previous CSV (keep attachments if re-run; rewrite as needed).
    fs::create_dir_all(output_dir)?;
    clean_previous_csv(output_dir)?;
    let attachments_dir = output_dir.join("attachments");
    fs::create_dir_all(&attachments_dir)?;

    let mut xml_paths: Vec<PathBuf> = fs::read_dir(input_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("xml"))
        })
        .collect();
    xml_paths.sort();

    for xml_path in xml_paths {
        match parse_xml_file(&xml_path) {
            Ok((msgs, stats)) => {
                report.xml_messages_seen += stats.messages;
                report.skipped_invalid_date += stats.skipped_invalid_date;
                report.skipped_unknown_type += stats.skipped_unknown_type;
                report.skipped_unknown_address += stats.skipped_unknown_address;
                add_xml_messages(&mut conversations, msgs);
            }
            Err(err) => report.errors.push(format!("{}: {err:#}", xml_path.display())),
        }
    }

    let mut pdu_paths: Vec<PathBuf> = fs::read_dir(input_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("I_") && n.ends_with(".pdu"))
        })
        .collect();
    pdu_paths.sort();

    for pdu_path in pdu_paths {
        match parse_pdu_file(&pdu_path, &owners.all_digits, &owners.primary_digits) {
            Ok(None) => {
                report.skipped_unparseable_pdu += 1;
                if report.errors.len() < 20 {
                    report
                        .errors
                        .push(format!("{}: unparseable PDU", pdu_path.display()));
                }
            }
            Ok(Some(parsed)) => match save_pdu_attachments(&parsed, &attachments_dir, &mut report)
            {
                Ok(atts) => add_pdu_message(
                    &mut conversations,
                    parsed,
                    atts,
                    &owners,
                    &mut report,
                ),
                Err(err) => report
                    .errors
                    .push(format!("{}: {err:#}", pdu_path.display())),
            },
            Err(err) => report.errors.push(format!("{}: {err:#}", pdu_path.display())),
        }
    }

    for (chat_id, mut convo) in conversations {
        write_conversation(output_dir, &chat_id, &mut convo, &mut report)?;
    }

    Ok(report)
}
