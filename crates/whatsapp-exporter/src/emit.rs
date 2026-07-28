//! Convert wtsexporter JSON → per-conversation vault-shaped CSV, EML, or MBOX.

use crate::cancel::{check_cancel, CancelFlag};
use crate::jid::{chat_id_from_jid, is_group_jid, jid_to_e164};
use crate::parse::{
    load_chat_store, media_path, message_text, timestamp_secs, ChatJson, MessageJson,
};
use anyhow::{Context, Result};
use message_csv::{
    conversation_filename, format_local_ts, json_cell, stable_guid, AttachmentCell, DateRange,
};
use message_exporters_core::OutputFormat;
use message_mail::{
    clean_previous_mail_output, write_mail_package, Direction as MailDirection, MailAttachment,
    MailMessage, MailPackage, Participant, SmsMailFields,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

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
    "whatsapp_jid",
    "whatsapp_key_id",
    "whatsapp_reply_json",
    "whatsapp_reactions_json",
];

const EXPORT_SOURCE: &str = "whatsapp";
const EXPORT_TOOL: &str = "WhatsApp Chat Exporter";
/// Pinned documented upstream version (JSON convert path; shell-out may differ).
pub const EXPORT_TOOL_VERSION: &str = "0.13.0";

#[derive(Debug, Default)]
pub struct ExportReport {
    pub conversations: u64,
    pub messages: u64,
    pub sent: u64,
    pub received: u64,
    pub attachments_saved: u64,
    pub attachments_missing: u64,
    pub skipped_invalid_date: u64,
    pub skipped_out_of_range: u64,
    pub errors: Vec<String>,
}

#[derive(Debug)]
struct PendingAttachment {
    rel_path: String,
    original_name: Option<String>,
    mime_type: Option<String>,
    is_sticker: bool,
    digest_hex: String,
}

#[derive(Debug)]
struct PendingMessage {
    sort_key: f64,
    is_from_me: bool,
    sender_handle: String,
    sender_display_name: String,
    text: String,
    key_id: String,
    reply_json: String,
    reactions_json: String,
    attachments: Vec<PendingAttachment>,
}

#[derive(Debug, Default)]
struct PendingConversation {
    conversation_type: String,
    group_title: Option<String>,
    whatsapp_jid: String,
    participant_e164s: Vec<String>,
    messages: Vec<PendingMessage>,
}

/// Convert a wtsexporter `result.json` into per-chat CSV under `output`.
///
/// `media_search_roots` are directories tried when resolving relative media paths
/// (typically the wtsexporter working directory / process cwd).
///
/// When `cancel` is set, it is checked between chats (cooperative cancellation).
pub fn convert_json(
    json_path: &Path,
    output: &Path,
    date_range: &DateRange,
    copy_attachments: bool,
    media_search_roots: &[PathBuf],
    output_format: OutputFormat,
    cancel: Option<&CancelFlag>,
) -> Result<ExportReport> {
    fs::create_dir_all(output).with_context(|| format!("create {}", output.display()))?;
    clean_previous_output(output)?;
    if copy_attachments {
        fs::create_dir_all(output.join("attachments"))?;
    }

    let store = load_chat_store(json_path)?;
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    for (jid, chat) in store {
        check_cancel(cancel)?;
        if jid.starts_with('_') {
            // Reserved / system keys if any.
            continue;
        }
        match ingest_chat(
            &jid,
            &chat,
            output,
            date_range,
            copy_attachments,
            media_search_roots,
            &mut report,
        ) {
            Ok(Some((chat_id, convo))) => {
                conversations.insert(chat_id, convo);
            }
            Ok(None) => {}
            Err(e) => report.errors.push(format!("{jid}: {e:#}")),
        }
    }

    for (chat_id, mut convo) in conversations {
        check_cancel(cancel)?;
        if !prepare_conversation(&mut convo, &mut report) {
            continue;
        }
        match output_format {
            OutputFormat::Csv => {
                write_conversation_csv(output, &chat_id, &convo, &mut report)?;
            }
            OutputFormat::Eml => {
                write_conversation_mail(
                    output,
                    &chat_id,
                    &convo,
                    MailPackage::EmlFolders,
                    &mut report,
                )?;
            }
            OutputFormat::Mbox => {
                write_conversation_mail(
                    output,
                    &chat_id,
                    &convo,
                    MailPackage::Mbox,
                    &mut report,
                )?;
            }
        }
        report.conversations += 1;
    }

    Ok(report)
}

fn ingest_chat(
    jid: &str,
    chat: &ChatJson,
    output: &Path,
    date_range: &DateRange,
    copy_attachments: bool,
    media_search_roots: &[PathBuf],
    report: &mut ExportReport,
) -> Result<Option<(String, PendingConversation)>> {
    let group = is_group_jid(jid);
    let chat_id = chat_id_from_jid(jid);
    let group_title = if group {
        chat.name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    } else {
        None
    };

    let mut peer_phones: BTreeSet<String> = BTreeSet::new();
    if !group {
        if let Some(e164) = jid_to_e164(jid) {
            peer_phones.insert(e164);
        }
    }

    let mut pending = PendingConversation {
        conversation_type: if group {
            "group".into()
        } else {
            "individual".into()
        },
        group_title,
        whatsapp_jid: jid.to_string(),
        participant_e164s: Vec::new(),
        messages: Vec::new(),
    };

    let display_fallback = chat.name.clone().unwrap_or_default();

    for (_id, msg) in &chat.messages {
        let Some(ts_raw) = msg.timestamp else {
            report.skipped_invalid_date += 1;
            continue;
        };
        let secs = timestamp_secs(ts_raw);
        if format_local_ts(secs).is_none() {
            report.skipped_invalid_date += 1;
            continue;
        }
        if !date_range.contains_secs(secs) {
            report.skipped_out_of_range += 1;
            continue;
        }

        let is_from_me = msg.from_me;
        let (sender_handle, sender_display_name) =
            resolve_sender(msg, is_from_me, &chat_id, &display_fallback, group);
        if group {
            if let Some(e164) = jid_to_e164(sender_handle.as_str())
                .or_else(|| msg.sender.as_deref().and_then(jid_to_e164))
            {
                peer_phones.insert(e164);
            }
        }

        let text = message_text(msg);
        let attachments = match media_path(msg) {
            Some(src) if copy_attachments => {
                match copy_media(
                    src,
                    chat.media_base.as_deref(),
                    media_search_roots,
                    output,
                    &chat_id,
                    msg,
                ) {
                    Ok(Some(att)) => {
                        report.attachments_saved += 1;
                        vec![att]
                    }
                    Ok(None) => {
                        report.attachments_missing += 1;
                        Vec::new()
                    }
                    Err(e) => {
                        report.errors.push(format!("{jid} media: {e:#}"));
                        Vec::new()
                    }
                }
            }
            Some(src) => vec![PendingAttachment {
                rel_path: src.to_string(),
                original_name: Path::new(src)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned()),
                mime_type: msg.mime.clone(),
                is_sticker: msg.sticker,
                digest_hex: digest_path_label(src),
            }],
            None => Vec::new(),
        };

        pending.messages.push(PendingMessage {
            sort_key: secs as f64,
            is_from_me,
            sender_handle,
            sender_display_name,
            text,
            key_id: key_id_string(msg),
            reply_json: optional_json(&msg.reply),
            reactions_json: reactions_json(&msg.reactions),
            attachments,
        });
    }

    if pending.messages.is_empty() {
        return Ok(None);
    }

    pending.participant_e164s = peer_phones.into_iter().collect();
    Ok(Some((chat_id, pending)))
}

fn resolve_sender(
    msg: &MessageJson,
    is_from_me: bool,
    chat_id: &str,
    chat_name: &str,
    group: bool,
) -> (String, String) {
    if is_from_me {
        return (String::new(), String::new());
    }
    if group {
        let handle = msg
            .sender
            .as_deref()
            .and_then(jid_to_e164)
            .or_else(|| msg.sender.clone())
            .unwrap_or_default();
        let display = msg
            .sender
            .as_deref()
            .filter(|s| jid_to_e164(s).is_none())
            .unwrap_or("")
            .to_string();
        (handle, display)
    } else {
        let handle = if chat_id.starts_with('+') {
            chat_id.to_string()
        } else {
            msg.sender
                .as_deref()
                .and_then(jid_to_e164)
                .unwrap_or_else(|| chat_id.to_string())
        };
        (handle, chat_name.to_string())
    }
}

fn copy_media(
    src: &str,
    media_base: Option<&str>,
    media_search_roots: &[PathBuf],
    output: &Path,
    chat_id: &str,
    msg: &MessageJson,
) -> Result<Option<PendingAttachment>> {
    let Some(src_path) = resolve_media_file(src, media_base, media_search_roots) else {
        return Ok(None);
    };
    let original = src_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "attachment.bin".into());
    let stem = sanitize_att_stem(chat_id);
    let dest_name = unique_name(output, &stem, &original, msg);
    let rel = format!("attachments/{dest_name}");
    let dest = output.join(&rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&src_path, &dest)
        .with_context(|| format!("copy {} → {}", src_path.display(), dest.display()))?;
    let digest = file_sha256(&dest)?;
    Ok(Some(PendingAttachment {
        rel_path: rel,
        original_name: Some(original),
        mime_type: msg.mime.clone(),
        is_sticker: msg.sticker,
        digest_hex: digest,
    }))
}

/// Resolve a wtsexporter media path against `media_base` and search roots.
fn resolve_media_file(
    src: &str,
    media_base: Option<&str>,
    media_search_roots: &[PathBuf],
) -> Option<PathBuf> {
    let hint = Path::new(src);
    if hint.is_file() {
        return Some(hint.to_path_buf());
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(base) = media_base.map(str::trim).filter(|s| !s.is_empty()) {
        let base_path = Path::new(base);
        candidates.push(base_path.join(hint));
        for root in media_search_roots {
            candidates.push(root.join(base_path).join(hint));
            if base_path.is_absolute() {
                candidates.push(base_path.join(hint));
            }
        }
    }
    for root in media_search_roots {
        candidates.push(root.join(hint));
    }

    candidates.into_iter().find(|p| p.is_file())
}

fn unique_name(output: &Path, chat_stem: &str, original: &str, msg: &MessageJson) -> String {
    let base = format!("{chat_stem}_{original}");
    let candidate = output.join("attachments").join(&base);
    if !candidate.exists() {
        return base;
    }
    let suffix = key_id_string(msg);
    let short = if suffix.len() > 12 {
        &suffix[..12]
    } else {
        &suffix
    };
    format!("{chat_stem}_{short}_{original}")
}

fn sanitize_att_stem(chat_id: &str) -> String {
    let s: String = chat_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "chat".into()
    } else {
        s
    }
}

fn file_sha256(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn digest_path_label(src: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(src.as_bytes());
    hex::encode(hasher.finalize())
}

fn key_id_string(msg: &MessageJson) -> String {
    match &msg.key_id {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

fn optional_json(v: &Option<serde_json::Value>) -> String {
    match v {
        Some(val) if !val.is_null() => json_cell(val),
        _ => String::new(),
    }
}

fn reactions_json(v: &serde_json::Value) -> String {
    if v.is_null() || (v.is_object() && v.as_object().is_some_and(|o| o.is_empty())) {
        String::new()
    } else {
        json_cell(v)
    }
}

fn clean_previous_output(output_dir: &Path) -> Result<()> {
    for entry in fs::read_dir(output_dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_file() && (name.ends_with(".csv") || name.ends_with(".csv.tmp")) {
            fs::remove_file(&path)
                .with_context(|| format!("remove previous {}", path.display()))?;
        }
    }
    clean_previous_mail_output(output_dir)?;
    Ok(())
}

fn prepare_conversation(convo: &mut PendingConversation, report: &mut ExportReport) -> bool {
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
    !convo.messages.is_empty()
}

fn pending_to_mail_messages(
    output_dir: &Path,
    chat_id: &str,
    convo: &PendingConversation,
    report: &mut ExportReport,
) -> Result<Vec<MailMessage>> {
    let participants: Vec<Participant> = convo
        .participant_e164s
        .iter()
        .filter(|h| !h.is_empty())
        .map(|h| Participant {
            handle: h.clone(),
            display_name: None,
        })
        .collect();

    let mut out = Vec::with_capacity(convo.messages.len());
    for msg in &convo.messages {
        report.messages += 1;
        if msg.is_from_me {
            report.sent += 1;
        } else {
            report.received += 1;
        }
        let secs = msg.sort_key as i64;
        let (ts_local, _, _) = format_local_ts(secs).expect("timestamp validated above");
        let digests: Vec<String> = msg.attachments.iter().map(|a| a.digest_hex.clone()).collect();
        let guid = stable_guid(chat_id, &ts_local, msg.is_from_me, &msg.text, &digests);
        let attachments: Vec<MailAttachment> = msg
            .attachments
            .iter()
            .map(|a| {
                MailAttachment::read_file(
                    &output_dir.join(&a.rel_path),
                    a.original_name.clone(),
                    a.mime_type.clone(),
                    Some(a.digest_hex.clone()),
                    a.is_sticker,
                )
            })
            .collect::<Result<_>>()?;
        let message_kind = if msg.attachments.is_empty() {
            "sms"
        } else {
            "mms"
        };
        let mut source = serde_json::Map::new();
        if !convo.whatsapp_jid.is_empty() {
            source.insert(
                "whatsapp_jid".into(),
                serde_json::Value::String(convo.whatsapp_jid.clone()),
            );
        }
        if !msg.key_id.is_empty() {
            source.insert(
                "whatsapp_key_id".into(),
                serde_json::Value::String(msg.key_id.clone()),
            );
        }
        if !msg.reply_json.is_empty() {
            source.insert(
                "reply_json".into(),
                serde_json::from_str(&msg.reply_json)
                    .unwrap_or_else(|_| serde_json::Value::String(msg.reply_json.clone())),
            );
        }
        if !msg.reactions_json.is_empty() {
            source.insert(
                "reactions_json".into(),
                serde_json::from_str(&msg.reactions_json)
                    .unwrap_or_else(|_| serde_json::Value::String(msg.reactions_json.clone())),
            );
        }
        let source_fields_json = if source.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(source).to_string())
        };

        out.push(MailMessage::sms(SmsMailFields {
            chat_identifier: chat_id.to_string(),
            conversation_type: convo.conversation_type.clone(),
            group_title: convo.group_title.clone(),
            participants: participants.clone(),
            guid,
            timestamp_unix_ms: secs.saturating_mul(1000),
            direction: if msg.is_from_me {
                MailDirection::Outgoing
            } else {
                MailDirection::Incoming
            },
            service: "WhatsApp".into(),
            message_kind: message_kind.into(),
            sender_handle: if msg.is_from_me || msg.sender_handle.is_empty() {
                None
            } else {
                Some(msg.sender_handle.clone())
            },
            sender_display_name: if msg.sender_display_name.is_empty() {
                None
            } else {
                Some(msg.sender_display_name.clone())
            },
            owner_handle: String::new(),
            subject: None,
            text: msg.text.clone(),
            android_type: None,
            source_fields_json,
            export_source: EXPORT_SOURCE.into(),
            export_tool: EXPORT_TOOL.into(),
            export_tool_version: EXPORT_TOOL_VERSION.into(),
            attachments,
            filename_suffix: Some("__whatsapp".into()),
        }));
    }
    Ok(out)
}

fn write_conversation_csv(
    output_dir: &Path,
    chat_id: &str,
    convo: &PendingConversation,
    report: &mut ExportReport,
) -> Result<()> {
    let filename = conversation_filename(
        &convo.conversation_type,
        chat_id,
        convo.group_title.as_deref(),
        &convo.participant_e164s,
        Some("__whatsapp"),
    );
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
        let attachment_cells: Vec<AttachmentCell> = msg
            .attachments
            .iter()
            .map(|a| AttachmentCell {
                path: Some(a.rel_path.clone()),
                original_name: a.original_name.clone(),
                mime_type: a.mime_type.clone(),
                is_sticker: a.is_sticker,
                transcription: None,
                sticker_effect: None,
            })
            .collect();
        let attachments_json = if attachment_cells.is_empty() {
            String::new()
        } else {
            json_cell(&attachment_cells)
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
            "WhatsApp",
            msg.sender_handle.as_str(),
            msg.sender_display_name.as_str(),
            msg.text.as_str(),
            attachments_json.as_str(),
            EXPORT_SOURCE,
            EXPORT_TOOL,
            EXPORT_TOOL_VERSION,
            convo.whatsapp_jid.as_str(),
            msg.key_id.as_str(),
            msg.reply_json.as_str(),
            msg.reactions_json.as_str(),
        ])
        .with_context(|| format!("write row {}", path.display()))?;

        report.messages += 1;
        if msg.is_from_me {
            report.sent += 1;
        } else {
            report.received += 1;
        }
    }

    wtr.flush()?;
    // Ensure file is closed before rename on Windows.
    drop(wtr);
    fs::rename(&tmp_path, &path)
        .with_context(|| format!("rename {} → {}", tmp_path.display(), path.display()))?;
    Ok(())
}

fn write_conversation_mail(
    output_dir: &Path,
    chat_id: &str,
    convo: &PendingConversation,
    package: MailPackage,
    report: &mut ExportReport,
) -> Result<()> {
    let messages = pending_to_mail_messages(output_dir, chat_id, convo, report)?;
    write_mail_package(output_dir, package, &messages)?;
    Ok(())
}
