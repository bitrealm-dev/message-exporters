//! Convert SMS Backup & Restore XML → canonical IR → CSV / EML / MBOX / JSON.

use crate::cancel::{check_cancel, CancelFlag};
use crate::xml::{parse_xml_file, AttachmentBlob, ConvType, ParsedMessage};
use anyhow::{bail, Result};
use message_contacts::ContactsBook;
use message_csv::{format_local_ts, json_cell, stable_guid, DateRange};
use message_exporters_core::OutputFormat;
use message_ir::{
    write_format, ConversationDocument, ConversationMeta, ExportMeta, IrAttachment, IrDirection,
    IrMessage, IrParticipant, SCHEMA_VERSION,
};
use message_mail::clean_previous_mail_output;
use message_phone::{to_e164, OwnerPhoneSet};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const EXPORT_SOURCE: &str = "sms-backup-restore";
const EXPORT_TOOL: &str = "SMS Backup & Restore";
const EXPORT_TOOL_VERSION: &str = "10.26.003";

#[derive(Debug, Default)]
pub struct ExportReport {
    pub conversations: u64,
    /// SMS elements seen in XML (before skip/dedupe filters).
    pub sms_seen: u64,
    /// MMS elements seen in XML (before skip/dedupe filters).
    pub mms_seen: u64,
    pub attachments_saved: u64,
    /// Rows written after dedupe (outgoing).
    pub sent: u64,
    /// Rows written after dedupe (incoming).
    pub received: u64,
    pub skipped_invalid_date: u64,
    pub skipped_out_of_range: u64,
    pub skipped_unknown_address: u64,
    pub skipped_unknown_type: u64,
    pub skipped_draft_or_outbox: u64,
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
    /// Kept in memory for EML embedding (avoid re-reading `attachments/`).
    bytes: Option<Arc<[u8]>>,
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
    participant_e164s: Vec<String>,
    messages: Vec<PendingMessage>,
}

fn chat_id_for(msg: &ParsedMessage) -> String {
    match msg.conversation_type {
        ConvType::Group => format!("chat-{}", msg.chat_key),
        ConvType::Individual => to_e164(&msg.chat_key),
    }
}

fn write_attachments(
    blobs: &[AttachmentBlob],
    attachments_dir: &Path,
    report: &mut ExportReport,
    copy_attachments: bool,
    keep_bytes: bool,
) -> Result<Vec<PendingAttachment>> {
    if !copy_attachments && !keep_bytes {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(blobs.len());
    for blob in blobs {
        if copy_attachments {
            let path = attachments_dir.join(&blob.filename);
            if !path.exists() {
                fs::write(&path, blob.data.as_ref())?;
                report.attachments_saved += 1;
            }
        }
        out.push(PendingAttachment {
            rel_path: format!("attachments/{}", blob.filename),
            original_name: blob.original_name.clone(),
            mime_type: blob.mime_type.clone(),
            digest_hex: blob.digest_hex.clone(),
            bytes: if keep_bytes {
                Some(Arc::clone(&blob.data))
            } else {
                None
            },
        });
    }
    Ok(out)
}

fn ensure_convo<'a>(
    map: &'a mut BTreeMap<String, PendingConversation>,
    chat_id: &str,
    conversation_type: ConvType,
    group_title: Option<String>,
    participant_e164s: Vec<String>,
) -> &'a mut PendingConversation {
    map.entry(chat_id.to_string())
        .or_insert_with(|| PendingConversation {
            conversation_type,
            group_title,
            participant_e164s,
            messages: Vec::new(),
        })
}

fn add_message(
    conversations: &mut BTreeMap<String, PendingConversation>,
    msg: ParsedMessage,
    pending_atts: Vec<PendingAttachment>,
) {
    let chat_id = chat_id_for(&msg);
    let peers: Vec<String> = msg
        .participant_digits
        .iter()
        .map(|(d, _)| to_e164(d))
        .filter(|d| !d.is_empty())
        .collect();
    let convo = ensure_convo(
        conversations,
        &chat_id,
        msg.conversation_type,
        msg.group_title.clone(),
        peers,
    );
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

fn prepare_conversation(
    convo: &mut PendingConversation,
    report: &mut ExportReport,
) -> Result<bool> {
    dedupe_messages(&mut convo.messages);
    convo.messages.retain(|m| {
        if format_local_ts(m.sort_key as i64).is_some() {
            true
        } else {
            report.skipped_invalid_date += 1;
            false
        }
    });
    Ok(!convo.messages.is_empty())
}

fn display_names_for_handles(convo: &PendingConversation) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for msg in &convo.messages {
        if let Some(digits) = &msg.sender_digits {
            let handle = to_e164(digits);
            if let Some(name) = msg
                .sender_display_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
            {
                names.entry(handle).or_insert_with(|| name.to_string());
            }
        }
        if convo.conversation_type == ConvType::Individual {
            let name = msg.contact_name.trim();
            if !name.is_empty() {
                for peer in &convo.participant_e164s {
                    names.entry(peer.clone()).or_insert_with(|| name.to_string());
                }
            }
        }
    }
    names
}

fn pending_to_document(
    chat_id: &str,
    convo: &PendingConversation,
    owner_handle: &str,
    report: &mut ExportReport,
) -> Result<ConversationDocument> {
    let conv_type = match convo.conversation_type {
        ConvType::Group => "group",
        ConvType::Individual => "individual",
    };
    let name_by_handle = display_names_for_handles(convo);
    let participants: Vec<IrParticipant> = convo
        .participant_e164s
        .iter()
        .filter(|h| !h.is_empty())
        .map(|h| IrParticipant {
            handle: h.clone(),
            display_name: name_by_handle.get(h).cloned(),
        })
        .collect();

    let mut messages = Vec::with_capacity(convo.messages.len());
    for msg in &convo.messages {
        if msg.is_from_me {
            report.sent += 1;
        } else {
            report.received += 1;
        }
        let secs = msg.sort_key as i64;
        let (ts_local, _, _) = format_local_ts(secs).expect("timestamp validated above");
        let digests: Vec<String> = msg.attachments.iter().map(|a| a.digest_hex.clone()).collect();
        let guid = stable_guid(chat_id, &ts_local, msg.is_from_me, &msg.text, &digests);
        let timestamp_unix_ms = msg
            .date_ms
            .parse::<i64>()
            .unwrap_or_else(|_| secs.saturating_mul(1000));
        let (sender_handle, sender_display_name) = if msg.is_from_me {
            (None, None)
        } else {
            (
                msg.sender_digits.as_ref().map(|d| to_e164(d)),
                msg.sender_display_name.clone(),
            )
        };
        let attachments: Vec<IrAttachment> = msg
            .attachments
            .iter()
            .map(|a| IrAttachment {
                path: Some(a.rel_path.clone()),
                original_name: a.original_name.clone(),
                mime_type: a.mime_type.clone(),
                digest_sha256: Some(a.digest_hex.clone()),
                is_sticker: false,
                transcription: None,
                sticker_effect: None,
                bytes: a.bytes.as_ref().map(|b| b.as_ref().to_vec()),
            })
            .collect();

        let mut source = serde_json::Map::new();
        if !msg.date_ms.is_empty() {
            source.insert("date_ms".into(), serde_json::json!(msg.date_ms));
        }
        if !msg.contact_name.is_empty() {
            source.insert("contact_name".into(), serde_json::json!(msg.contact_name));
        }
        if !msg.android_type.is_empty() {
            source.insert("android_type".into(), serde_json::json!(msg.android_type));
        }
        if !msg.xml_fields_json.is_empty() {
            source.insert(
                "xml_fields_json".into(),
                serde_json::json!(msg.xml_fields_json),
            );
        }

        messages.push(IrMessage {
            guid,
            timestamp_unix_ms,
            direction: if msg.is_from_me {
                IrDirection::Outgoing
            } else {
                IrDirection::Incoming
            },
            service: "SMS".into(),
            message_kind: msg.message_kind.to_string(),
            sender_handle,
            sender_display_name,
            subject: if msg.subject.is_empty() {
                None
            } else {
                Some(msg.subject.clone())
            },
            text: msg.text.clone(),
            attachments,
            imessage: None,
            source: if source.is_empty() {
                None
            } else {
                Some(serde_json::Value::Object(source))
            },
        });
    }

    Ok(ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export: ExportMeta {
            source: EXPORT_SOURCE.into(),
            tool: EXPORT_TOOL.into(),
            tool_version: EXPORT_TOOL_VERSION.into(),
            owner_handle: Some(owner_handle.to_string()),
            owner_display_name: None,
        },
        conversation: ConversationMeta {
            chat_identifier: chat_id.to_string(),
            conversation_type: conv_type.to_string(),
            group_title: convo.group_title.clone(),
            participants,
            filename_suffix: None,
        },
        messages,
    })
}

fn clean_previous_output(output_dir: &Path) -> Result<()> {
    for entry in fs::read_dir(output_dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_file()
            && (name.ends_with(".csv") || name.ends_with(".csv.tmp") || name.ends_with(".json"))
        {
            let _ = fs::remove_file(&path);
        }
    }
    clean_previous_mail_output(output_dir)?;
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

fn enrich_pending_names(book: &ContactsBook, chat_id: &str, msg: &mut PendingMessage) {
    let phones: Vec<&str> = msg
        .sender_digits
        .as_deref()
        .into_iter()
        .chain(std::iter::once(chat_id))
        .collect();
    for phone in phones {
        if let Some(name) = book.enrich_display_name(phone, &msg.contact_name) {
            msg.contact_name = name;
        }
        let cur = msg.sender_display_name.as_deref().unwrap_or("");
        if let Some(name) = book.enrich_display_name(phone, cur) {
            msg.sender_display_name = Some(name);
        }
    }
}

/// Convert SMS Backup & Restore XML into IR, then CSV / EML / MBOX / JSON.
///
/// When `cancel` is set, cooperative cancellation is checked between XML files
/// and before writing. Cancelled runs return an error with message `cancelled`.
pub fn convert_export(
    input: &Path,
    output_dir: &Path,
    owner_phones: &[String],
    contacts: &ContactsBook,
    date_range: &DateRange,
    copy_attachments: bool,
    output_format: OutputFormat,
    cancel: Option<&CancelFlag>,
) -> Result<ExportReport> {
    let owners = OwnerPhoneSet::new(owner_phones)?;
    let owner_handle = to_e164(&owners.primary_digits);
    let keep_bytes = output_format.is_mail_archive();
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    fs::create_dir_all(output_dir)?;
    clean_previous_output(output_dir)?;
    let attachments_dir = output_dir.join("attachments");
    if copy_attachments {
        fs::create_dir_all(&attachments_dir)?;
    }

    for xml_path in collect_xml_paths(input)? {
        check_cancel(cancel)?;
        match parse_xml_file(&xml_path, &owners.all_digits) {
            Ok((msgs, stats)) => {
                report.sms_seen += stats.sms_seen;
                report.mms_seen += stats.mms_seen;
                report.skipped_invalid_date += stats.skipped_invalid_date;
                report.skipped_unknown_address += stats.skipped_unknown_address;
                report.skipped_unknown_type += stats.skipped_unknown_type;
                report.skipped_draft_or_outbox += stats.skipped_draft_or_outbox;
                report.skipped_empty_participants += stats.skipped_empty_participants;
                report.skipped_bad_attachment += stats.skipped_bad_attachment;
                for msg in msgs {
                    if !date_range.contains_secs_f64(msg.timestamp_secs) {
                        report.skipped_out_of_range += 1;
                        continue;
                    }
                    match write_attachments(
                        &msg.attachments,
                        &attachments_dir,
                        &mut report,
                        copy_attachments,
                        keep_bytes,
                    ) {
                        Ok(atts) => add_message(&mut conversations, msg, atts),
                        Err(err) => report
                            .errors
                            .push(format!("{}: {err:#}", xml_path.display())),
                    }
                }
            }
            Err(err) => report.errors.push(format!("{}: {err:#}", xml_path.display())),
        }
    }

    check_cancel(cancel)?;

    for (chat_id, mut convo) in conversations {
        for msg in &mut convo.messages {
            enrich_pending_names(contacts, &chat_id, msg);
        }
        if !prepare_conversation(&mut convo, &mut report)? {
            continue;
        }
        let doc = pending_to_document(&chat_id, &convo, &owner_handle, &mut report)?;
        write_format(output_dir, output_format, &doc)?;
        report.conversations += 1;
    }

    Ok(report)
}
