//! Canonical conversation intermediate representation (IR).
//!
//! Source exporters parse vendor formats into [`ConversationDocument`], then
//! project with [`write_format`] to CSV, EML, MBOX, JSON, or JSONL. See
//! [`docs/MESSAGE_IR.md`](../../../docs/MESSAGE_IR.md).
//!
//! Schema version 2 nests vendor/iMessage bags as real JSON values (not
//! stringified `*_json` cells). CSV/EML projectors stringify at the boundary.

use anyhow::{bail, Context, Result};
use message_csv::{
    conversation_filename, format_local_ts, json_cell, AttachmentCell,
};
use message_exporters_core::OutputFormat;
use message_mail::{
    write_mail_package, Direction as MailDirection, MailAttachment, MailMessage, MailPackage,
    Participant, SmsMailFields,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 2;

/// Shared CSV columns written by the IR CSV projector (SBR-compatible).
pub const CSV_HEADERS: &[&str] = &[
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
    "message_kind",
    "date_ms",
    "contact_name",
    "android_type",
    "xml_fields_json",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationDocument {
    pub schema_version: u32,
    pub export: ExportMeta,
    pub conversation: ConversationMeta,
    pub messages: Vec<IrMessage>,
}

/// Shared CSV columns written by the IR CSV projector for `export.source == "imessage"`.
pub const IMESSAGE_CSV_HEADERS: &[&str] = &[
    "chat_identifier",
    "conversation_type",
    "group_title",
    "participants_json",
    "guid",
    "timestamp",
    "timestamp_utc",
    "timestamp_display",
    "read_receipt",
    "direction",
    "service",
    "sender_handle",
    "sender_display_name",
    "subject",
    "text",
    "is_deleted",
    "send_effect",
    "shared_location",
    "is_announcement",
    "announcement",
    "is_reply",
    "thread_originator_guid",
    "thread_originator_part",
    "num_replies",
    "parts_json",
    "edits_json",
    "attachments_json",
    "tapbacks_json",
    "app_json",
    "export_source",
    "export_tool",
    "export_tool_version",
];

const IMESSAGE_EXPORT_SOURCE: &str = "imessage";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMeta {
    pub source: String,
    pub tool: String,
    pub tool_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_handle: Option<String>,
    /// Outgoing From display name (iMessage `--use-caller-id`); defaults to `"Me"` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_display_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IrConversationType {
    Individual,
    Group,
}

impl IrConversationType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Individual => "individual",
            Self::Group => "group",
        }
    }

    /// Normalize common labels; unknown values become [`Self::Individual`].
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "group" => Self::Group,
            _ => Self::Individual,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub chat_identifier: String,
    pub conversation_type: IrConversationType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<IrParticipant>,
    /// Packaging stem suffix (e.g. `__whatsapp`). Serialized when present so
    /// the JSON body matches the on-disk filename stem.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename_suffix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrParticipant {
    pub handle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrMessage {
    pub guid: String,
    pub timestamp_unix_ms: i64,
    pub direction: IrDirection,
    pub service: String,
    pub message_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<IrAttachment>,
    /// iMessage extensions. Nested keys: `parts`, `edits`, `tapbacks`, `app`
    /// (arrays/objects), plus scalar flags. Omitted for SMS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imessage: Option<Value>,
    /// Vendor leftovers: `contact_name`, `android_type`, `fields` (object).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IrDirection {
    Incoming,
    Outgoing,
}

impl IrDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::Outgoing => "outgoing",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrAttachment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest_sha256: Option<String>,
    #[serde(default)]
    pub is_sticker: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcription: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sticker_effect: Option<String>,
    /// In-memory bytes for EML embedding; never written to JSON.
    #[serde(skip)]
    pub bytes: Option<Vec<u8>>,
}

impl ConversationDocument {
    pub fn filename_stem(&self) -> String {
        let handles: Vec<String> = self
            .conversation
            .participants
            .iter()
            .map(|p| p.handle.clone())
            .collect();
        let csv = conversation_filename(
            self.conversation.conversation_type.as_str(),
            &self.conversation.chat_identifier,
            self.conversation.group_title.as_deref(),
            &handles,
            self.conversation.filename_suffix.as_deref(),
        );
        csv.strip_suffix(".csv")
            .unwrap_or(csv.as_str())
            .to_string()
    }
}

/// Write one conversation in the requested packaging format.
pub fn write_format(
    output_dir: &Path,
    format: OutputFormat,
    doc: &ConversationDocument,
) -> Result<PathBuf> {
    match format {
        OutputFormat::Csv => write_conversation_csv(output_dir, doc),
        OutputFormat::Json => write_conversation_json(output_dir, doc),
        OutputFormat::Jsonl => write_conversation_jsonl(output_dir, doc),
        OutputFormat::Eml => write_conversation_mail(output_dir, doc, MailPackage::EmlFolders),
        OutputFormat::Mbox => write_conversation_mail(output_dir, doc, MailPackage::Mbox),
    }
}

/// Per-conversation JSON artifact (`<stem>.json`).
pub fn write_conversation_json(output_dir: &Path, doc: &ConversationDocument) -> Result<PathBuf> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("create {}", output_dir.display()))?;
    let path = output_dir.join(format!("{}.json", doc.filename_stem()));
    let mut tmp = path.clone();
    tmp.set_extension("json.tmp");
    let json = serde_json::to_vec_pretty(doc).context("serialize ConversationDocument")?;
    {
        let mut file =
            File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        file.write_all(&json)
            .with_context(|| format!("write {}", tmp.display()))?;
        file.write_all(b"\n")?;
    }
    fs::rename(&tmp, &path)
        .with_context(|| format!("rename {} → {}", tmp.display(), path.display()))?;
    Ok(path)
}

/// JSONL header line (schema + export + conversation; no messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonlHeader {
    schema_version: u32,
    export: ExportMeta,
    conversation: ConversationMeta,
}

/// Per-conversation JSON Lines (`<stem>.jsonl`): header object, then one
/// [`IrMessage`] per line.
pub fn write_conversation_jsonl(output_dir: &Path, doc: &ConversationDocument) -> Result<PathBuf> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("create {}", output_dir.display()))?;
    let path = output_dir.join(format!("{}.jsonl", doc.filename_stem()));
    let mut tmp = path.clone();
    tmp.set_extension("jsonl.tmp");
    {
        let mut file =
            File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        let header = JsonlHeader {
            schema_version: doc.schema_version,
            export: doc.export.clone(),
            conversation: doc.conversation.clone(),
        };
        serde_json::to_writer(&mut file, &header).context("serialize JSONL header")?;
        file.write_all(b"\n")?;
        for msg in &doc.messages {
            serde_json::to_writer(&mut file, msg).context("serialize JSONL message")?;
            file.write_all(b"\n")?;
        }
    }
    fs::rename(&tmp, &path)
        .with_context(|| format!("rename {} → {}", tmp.display(), path.display()))?;
    Ok(path)
}

/// Per-conversation CSV. Dispatches to the iMessage-shaped projector for
/// `export.source == "imessage"` (fork-compatible columns); otherwise the
/// shared SBR-style core + `source` columns.
pub fn write_conversation_csv(output_dir: &Path, doc: &ConversationDocument) -> Result<PathBuf> {
    if doc.export.source == IMESSAGE_EXPORT_SOURCE {
        write_conversation_csv_imessage(output_dir, doc)
    } else {
        write_conversation_csv_core(output_dir, doc)
    }
}

fn write_conversation_csv_core(output_dir: &Path, doc: &ConversationDocument) -> Result<PathBuf> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("create {}", output_dir.display()))?;
    let filename = conversation_filename(
        doc.conversation.conversation_type.as_str(),
        &doc.conversation.chat_identifier,
        doc.conversation.group_title.as_deref(),
        &doc.conversation
            .participants
            .iter()
            .map(|p| p.handle.clone())
            .collect::<Vec<_>>(),
        doc.conversation.filename_suffix.as_deref(),
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
    wtr.write_record(CSV_HEADERS)
        .with_context(|| format!("write header {}", path.display()))?;

    for msg in &doc.messages {
        let secs = msg.timestamp_unix_ms.div_euclid(1000);
        let (ts_local, ts_utc, ts_display) = format_local_ts(secs).ok_or_else(|| {
            anyhow::anyhow!("invalid timestamp_unix_ms {}", msg.timestamp_unix_ms)
        })?;
        let attachment_cells: Vec<AttachmentCell> = msg
            .attachments
            .iter()
            .map(|a| AttachmentCell {
                path: a.path.clone(),
                original_name: a.original_name.clone(),
                mime_type: a.mime_type.clone(),
                is_sticker: a.is_sticker,
                transcription: a.transcription.clone(),
                sticker_effect: a.sticker_effect.clone(),
            })
            .collect();
        let attachments_json = json_cell(&attachment_cells);
        let source = msg.source.as_ref();
        let date_ms = msg.timestamp_unix_ms.to_string();
        let contact_name = source_string(source, "contact_name").unwrap_or_default();
        let android_type = source_string(source, "android_type").unwrap_or_default();
        let xml_fields_json = source_fields_for_csv(source);

        wtr.write_record([
            doc.conversation.chat_identifier.as_str(),
            doc.conversation.conversation_type.as_str(),
            doc.conversation.group_title.as_deref().unwrap_or(""),
            msg.guid.as_str(),
            ts_local.as_str(),
            ts_utc.as_str(),
            ts_display.as_str(),
            msg.direction.as_str(),
            msg.service.as_str(),
            msg.sender_handle.as_deref().unwrap_or(""),
            msg.sender_display_name.as_deref().unwrap_or(""),
            msg.subject.as_deref().unwrap_or(""),
            msg.text.as_str(),
            attachments_json.as_str(),
            doc.export.source.as_str(),
            doc.export.tool.as_str(),
            doc.export.tool_version.as_str(),
            msg.message_kind.as_str(),
            date_ms.as_str(),
            contact_name.as_str(),
            android_type.as_str(),
            xml_fields_json.as_str(),
        ])
        .with_context(|| format!("write row {}", path.display()))?;
    }

    wtr.flush()?;
    drop(wtr);
    fs::rename(&tmp_path, &path)
        .with_context(|| format!("rename {} → {}", tmp_path.display(), path.display()))?;
    Ok(path)
}

fn source_string(source: Option<&Value>, key: &str) -> Option<String> {
    source
        .and_then(|v| v.get(key))
        .and_then(|v| {
            v.as_str()
                .map(str::to_string)
                .or_else(|| v.as_i64().map(|n| n.to_string()))
        })
        .filter(|s| !s.is_empty())
}

/// Serialize nested `source.fields` for the CSV `xml_fields_json` cell.
fn source_fields_for_csv(source: Option<&Value>) -> String {
    source
        .and_then(|s| s.get("fields"))
        .map(|fields| serde_json::to_string(fields).unwrap_or_default())
        .unwrap_or_default()
}

fn imessage_str(imessage: Option<&Value>, key: &str) -> Option<String> {
    imessage
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn imessage_bool(imessage: Option<&Value>, key: &str) -> bool {
    imessage
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn imessage_u32(imessage: Option<&Value>, key: &str) -> Option<u32> {
    imessage
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_u64())
        .and_then(|n| u32::try_from(n).ok())
}

/// Stringify a nested iMessage bag value (`parts` / `edits` / …) for CSV/mail.
fn imessage_json_cell(imessage: Option<&Value>, nested_key: &str) -> String {
    imessage
        .and_then(|b| b.get(nested_key))
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "null".into()))
        .unwrap_or_else(|| "null".into())
}

/// Stringify a nested object/array from the iMessage bag for mail headers.
fn imessage_nested_as_string(imessage: &Value, nested_key: &str) -> Option<String> {
    let v = imessage.get(nested_key)?;
    if v.is_null() {
        return None;
    }
    Some(serde_json::to_string(v).unwrap_or_default()).filter(|s| !s.is_empty())
}

/// Parse a JSON string into a [`Value`], or return the string as a JSON string value.
pub fn parse_json_value(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|_| json!(s))
}

#[derive(Serialize)]
struct ImessageParticipantCell {
    handle: String,
    display_name: String,
}

/// Per-conversation CSV matching `IMESSAGE_CSV_HEADERS`, populated from core
/// IR fields plus the per-message `imessage` bag.
///
/// `read_receipt` carries the raw `read_receipt_rfc3339` value (human phrases
/// like `"(Read by you after 12 seconds)"` are not reconstructable from IR alone).
fn write_conversation_csv_imessage(output_dir: &Path, doc: &ConversationDocument) -> Result<PathBuf> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("create {}", output_dir.display()))?;
    let filename = conversation_filename(
        doc.conversation.conversation_type.as_str(),
        &doc.conversation.chat_identifier,
        doc.conversation.group_title.as_deref(),
        &doc.conversation
            .participants
            .iter()
            .map(|p| p.handle.clone())
            .collect::<Vec<_>>(),
        doc.conversation.filename_suffix.as_deref(),
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
    wtr.write_record(IMESSAGE_CSV_HEADERS)
        .with_context(|| format!("write header {}", path.display()))?;

    let participants_json = json_cell(
        &doc.conversation
            .participants
            .iter()
            .map(|p| ImessageParticipantCell {
                handle: p.handle.clone(),
                display_name: p.display_name.clone().unwrap_or_default(),
            })
            .collect::<Vec<_>>(),
    );

    for msg in &doc.messages {
        let secs = msg.timestamp_unix_ms.div_euclid(1000);
        let (ts_local, ts_utc, ts_display) = format_local_ts(secs).ok_or_else(|| {
            anyhow::anyhow!("invalid timestamp_unix_ms {}", msg.timestamp_unix_ms)
        })?;
        let imessage = msg.imessage.as_ref();
        let read_receipt = imessage_str(imessage, "read_receipt_rfc3339").unwrap_or_default();
        let is_deleted = imessage_bool(imessage, "is_deleted");
        let send_effect = imessage_str(imessage, "send_effect").unwrap_or_default();
        let shared_location = imessage_str(imessage, "shared_location").unwrap_or_default();
        let is_announcement = msg.message_kind == "announcement";
        let announcement = imessage_str(imessage, "announcement").unwrap_or_default();
        let is_reply = imessage_bool(imessage, "is_reply");
        let thread_originator_guid = if is_reply {
            imessage_str(imessage, "in_reply_to_guid").unwrap_or_default()
        } else {
            String::new()
        };
        let thread_originator_part = if is_reply {
            imessage_u32(imessage, "thread_originator_part")
                .unwrap_or(0)
                .to_string()
        } else {
            String::new()
        };
        let num_replies = imessage_u32(imessage, "num_replies").unwrap_or(0).to_string();
        let parts_json = imessage_json_cell(imessage, "parts");
        let edits_json = imessage_json_cell(imessage, "edits");
        let tapbacks_json = imessage_json_cell(imessage, "tapbacks");
        let app_json = imessage_json_cell(imessage, "app");

        let attachment_cells: Vec<AttachmentCell> = msg
            .attachments
            .iter()
            .map(|a| AttachmentCell {
                path: a.path.clone(),
                original_name: a.original_name.clone(),
                mime_type: a.mime_type.clone(),
                is_sticker: a.is_sticker,
                transcription: a.transcription.clone(),
                sticker_effect: a.sticker_effect.clone(),
            })
            .collect();
        let attachments_json = json_cell(&attachment_cells);

        wtr.write_record([
            doc.conversation.chat_identifier.as_str(),
            doc.conversation.conversation_type.as_str(),
            doc.conversation.group_title.as_deref().unwrap_or(""),
            participants_json.as_str(),
            msg.guid.as_str(),
            ts_local.as_str(),
            ts_utc.as_str(),
            ts_display.as_str(),
            read_receipt.as_str(),
            msg.direction.as_str(),
            msg.service.as_str(),
            msg.sender_handle.as_deref().unwrap_or(""),
            msg.sender_display_name.as_deref().unwrap_or(""),
            msg.subject.as_deref().unwrap_or(""),
            msg.text.as_str(),
            if is_deleted { "true" } else { "false" },
            send_effect.as_str(),
            shared_location.as_str(),
            if is_announcement { "true" } else { "false" },
            announcement.as_str(),
            if is_reply { "true" } else { "false" },
            thread_originator_guid.as_str(),
            thread_originator_part.as_str(),
            num_replies.as_str(),
            parts_json.as_str(),
            edits_json.as_str(),
            attachments_json.as_str(),
            tapbacks_json.as_str(),
            app_json.as_str(),
            doc.export.source.as_str(),
            doc.export.tool.as_str(),
            doc.export.tool_version.as_str(),
        ])
        .with_context(|| format!("write row {}", path.display()))?;
    }

    wtr.flush()?;
    drop(wtr);
    fs::rename(&tmp_path, &path)
        .with_context(|| format!("rename {} → {}", tmp_path.display(), path.display()))?;
    Ok(path)
}

fn write_conversation_mail(
    output_dir: &Path,
    doc: &ConversationDocument,
    package: MailPackage,
) -> Result<PathBuf> {
    let messages = document_to_mail_messages(doc, output_dir)?;
    if messages.is_empty() {
        bail!("conversation has no messages");
    }
    write_mail_package(output_dir, package, &messages)
}

/// Build [`MailMessage`] list from IR (reads attachment bytes from disk when missing).
pub fn document_to_mail_messages(
    doc: &ConversationDocument,
    output_dir: &Path,
) -> Result<Vec<MailMessage>> {
    let owner = doc
        .export
        .owner_handle
        .clone()
        .unwrap_or_default();
    let participants: Vec<Participant> = doc
        .conversation
        .participants
        .iter()
        .map(|p| Participant {
            handle: p.handle.clone(),
            display_name: p.display_name.clone(),
        })
        .collect();

    let mut out = Vec::with_capacity(doc.messages.len());
    for msg in &doc.messages {
        let mut attachments = Vec::with_capacity(msg.attachments.len());
        for a in &msg.attachments {
            let bytes = if let Some(b) = &a.bytes {
                b.clone()
            } else if let Some(rel) = a.path.as_deref() {
                let path = output_dir.join(rel);
                if path.is_file() {
                    fs::read(&path).with_context(|| format!("read {}", path.display()))?
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
            attachments.push(MailAttachment {
                bytes,
                original_name: a.original_name.clone(),
                mime_type: a.mime_type.clone(),
                digest_sha256: a.digest_sha256.clone(),
                is_sticker: a.is_sticker,
                transcription: a.transcription.clone(),
                sticker_effect: a.sticker_effect.clone(),
            });
        }

        let android_type = source_string(msg.source.as_ref(), "android_type");
        let source_fields_json = {
            let s = source_fields_for_csv(msg.source.as_ref());
            (!s.is_empty()).then_some(s)
        };

        let mut mail = MailMessage::sms(SmsMailFields {
            chat_identifier: doc.conversation.chat_identifier.clone(),
            conversation_type: doc.conversation.conversation_type.as_str().to_string(),
            group_title: doc.conversation.group_title.clone(),
            participants: participants.clone(),
            guid: msg.guid.clone(),
            timestamp_unix_ms: msg.timestamp_unix_ms,
            direction: match msg.direction {
                IrDirection::Incoming => MailDirection::Incoming,
                IrDirection::Outgoing => MailDirection::Outgoing,
            },
            service: msg.service.clone(),
            message_kind: msg.message_kind.clone(),
            sender_handle: msg.sender_handle.clone(),
            sender_display_name: msg.sender_display_name.clone(),
            owner_handle: owner.clone(),
            subject: msg.subject.clone(),
            text: msg.text.clone(),
            android_type,
            source_fields_json,
            export_source: doc.export.source.clone(),
            export_tool: doc.export.tool.clone(),
            export_tool_version: doc.export.tool_version.clone(),
            attachments,
            filename_suffix: doc.conversation.filename_suffix.clone(),
        });
        if let Some(imessage) = &msg.imessage {
            apply_imessage_fields(&mut mail, imessage);
        }
        if mail.owner_display_name.is_none() {
            mail.owner_display_name = doc.export.owner_display_name.clone();
        }
        out.push(mail);
    }
    Ok(out)
}

/// Restore iMessage extension fields (parts, tapbacks, balloons, replies, …)
/// from an [`IrMessage::imessage`] bag onto a [`MailMessage`].
///
/// Unknown / absent keys leave the corresponding field unset. `owner_handle`
/// is not touched here (it comes from [`ExportMeta::owner_handle`]).
pub fn apply_imessage_fields(mail: &mut MailMessage, imessage: &Value) {
    let bag = Some(imessage);
    mail.is_reply = imessage_bool(bag, "is_reply");
    mail.in_reply_to_guid = imessage_str(bag, "in_reply_to_guid");
    mail.thread_originator_part = imessage_u32(bag, "thread_originator_part");
    mail.num_replies = imessage_u32(bag, "num_replies");
    mail.is_deleted = imessage_bool(bag, "is_deleted");
    mail.send_effect = imessage_str(bag, "send_effect");
    mail.shared_location = imessage_str(bag, "shared_location");
    mail.announcement = imessage_str(bag, "announcement");
    mail.read_receipt_rfc3339 = imessage_str(bag, "read_receipt_rfc3339");
    mail.parts_json = imessage_nested_as_string(imessage, "parts");
    mail.edits_json = imessage_nested_as_string(imessage, "edits");
    mail.app_json = imessage_nested_as_string(imessage, "app");
    mail.balloon_bundle_id = imessage_str(bag, "balloon_bundle_id");
    mail.balloon_kind = imessage_str(bag, "balloon_kind");
    mail.tapbacks_json = imessage_nested_as_string(imessage, "tapbacks");
    mail.associated_guid = imessage_str(bag, "associated_guid");
    mail.associated_part = imessage_u32(bag, "associated_part");
    mail.tapback_kind = imessage_str(bag, "tapback_kind");
    mail.tapback_emoji = imessage_str(bag, "tapback_emoji");
    mail.tapback_action = imessage_str(bag, "tapback_action");
    if let Some(name) = imessage_str(bag, "owner_display_name") {
        mail.owner_display_name = Some(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use message_mail::clean_previous_mail_output;

    fn sample_doc() -> ConversationDocument {
        ConversationDocument {
            schema_version: SCHEMA_VERSION,
            export: ExportMeta {
                source: "sms-backup-restore".into(),
                tool: "SMS Backup & Restore".into(),
                tool_version: "10.26.003".into(),
                owner_handle: Some("+15555550100".into()),
                owner_display_name: None,
            },
            conversation: ConversationMeta {
                chat_identifier: "+15555550101".into(),
                conversation_type: IrConversationType::Individual,
                group_title: None,
                participants: vec![IrParticipant {
                    handle: "+15555550101".into(),
                    display_name: Some("Sam".into()),
                }],
                filename_suffix: None,
            },
            messages: vec![IrMessage {
                guid: "aabbccddeeff00112233445566778899".into(),
                timestamp_unix_ms: 1_400_773_261_000,
                direction: IrDirection::Incoming,
                service: "sms".into(),
                message_kind: "sms".into(),
                sender_handle: Some("+15555550101".into()),
                sender_display_name: Some("Sam".into()),
                subject: None,
                text: "hello ir".into(),
                attachments: vec![],
                imessage: None,
                source: Some(serde_json::json!({
                    "contact_name": "Sam",
                    "android_type": "1",
                    "fields": { "address": "+15555550101" }
                })),
            }],
        }
    }

    #[test]
    fn writes_json_csv_jsonl_and_eml() {
        let tmp = tempfile::tempdir().unwrap();
        let doc = sample_doc();

        let json_path = write_format(tmp.path(), OutputFormat::Json, &doc).unwrap();
        assert!(json_path.ends_with("+15555550101.json"));
        let parsed: ConversationDocument =
            serde_json::from_slice(&fs::read(&json_path).unwrap()).unwrap();
        assert_eq!(parsed.schema_version, 2);
        assert_eq!(parsed.messages[0].text, "hello ir");
        assert!(parsed.messages[0].attachments.is_empty());
        assert!(parsed.messages[0]
            .source
            .as_ref()
            .unwrap()
            .get("fields")
            .unwrap()
            .is_object());
        assert!(parsed.messages[0]
            .source
            .as_ref()
            .unwrap()
            .get("date_ms")
            .is_none());

        let jsonl_path = write_format(tmp.path(), OutputFormat::Jsonl, &doc).unwrap();
        assert!(jsonl_path.ends_with("+15555550101.jsonl"));
        let jsonl = fs::read_to_string(&jsonl_path).unwrap();
        let mut lines = jsonl.lines();
        let header: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(header["schema_version"], 2);
        assert!(header.get("messages").is_none());
        let msg_line: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(msg_line["text"], "hello ir");
        assert!(msg_line["source"]["fields"].is_object());

        let csv_path = write_format(tmp.path(), OutputFormat::Csv, &doc).unwrap();
        let csv = fs::read_to_string(&csv_path).unwrap();
        assert!(csv.contains("hello ir"));
        assert!(csv.contains("sms-backup-restore"));
        assert!(csv.contains("xml_fields_json"));
        assert!(csv.contains("+15555550101"));

        let _ = clean_previous_mail_output(tmp.path());
        let eml_dir = write_format(tmp.path(), OutputFormat::Eml, &doc).unwrap();
        assert!(eml_dir.is_dir());
    }

    fn sample_imessage_doc() -> ConversationDocument {
        ConversationDocument {
            schema_version: SCHEMA_VERSION,
            export: ExportMeta {
                source: "imessage".into(),
                tool: "imessage-ir-exporter".into(),
                tool_version: "0.1.0".into(),
                owner_handle: Some("+15555550100".into()),
                owner_display_name: Some("Me".into()),
            },
            conversation: ConversationMeta {
                chat_identifier: "+15555550101".into(),
                conversation_type: IrConversationType::Individual,
                group_title: None,
                participants: vec![IrParticipant {
                    handle: "+15555550101".into(),
                    display_name: Some("Sam".into()),
                }],
                filename_suffix: None,
            },
            messages: vec![
                IrMessage {
                    guid: "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE".into(),
                    timestamp_unix_ms: 1_400_773_261_000,
                    direction: IrDirection::Incoming,
                    service: "imessage".into(),
                    message_kind: "imessage".into(),
                    sender_handle: Some("+15555550101".into()),
                    sender_display_name: Some("Sam".into()),
                    subject: None,
                    text: "hello imessage".into(),
                    attachments: vec![],
                    imessage: Some(serde_json::json!({
                        "is_reply": true,
                        "in_reply_to_guid": "parent-guid-1111",
                        "thread_originator_part": 0,
                        "num_replies": 2,
                        "is_deleted": false,
                        "send_effect": "Sent with Balloons",
                        "tapbacks": [{"part_index": 0, "kind": "loved"}],
                        "parts": [{"index": 0, "kind": "run", "text": "hello imessage"}],
                        "owner_display_name": "+15555550100",
                    })),
                    source: None,
                },
                IrMessage {
                    guid: "TAPBACK-GUID-0001".into(),
                    timestamp_unix_ms: 1_400_773_262_000,
                    direction: IrDirection::Outgoing,
                    service: "imessage".into(),
                    message_kind: "tapback".into(),
                    sender_handle: None,
                    sender_display_name: None,
                    subject: None,
                    text: "Loved a message".into(),
                    attachments: vec![],
                    imessage: Some(serde_json::json!({
                        "associated_guid": "parent-guid-1111",
                        "associated_part": 0,
                        "tapback_kind": "loved",
                        "tapback_action": "add",
                        "in_reply_to_guid": "parent-guid-1111",
                    })),
                    source: None,
                },
            ],
        }
    }

    #[test]
    fn imessage_bag_restores_mail_extension_headers() {
        let tmp = tempfile::tempdir().unwrap();
        let doc = sample_imessage_doc();
        let mail_messages = document_to_mail_messages(&doc, tmp.path()).unwrap();

        let reply = &mail_messages[0];
        assert!(reply.is_reply);
        assert_eq!(reply.in_reply_to_guid.as_deref(), Some("parent-guid-1111"));
        assert_eq!(reply.thread_originator_part, Some(0));
        assert_eq!(reply.num_replies, Some(2));
        assert_eq!(reply.send_effect.as_deref(), Some("Sent with Balloons"));
        assert!(reply.tapbacks_json.as_deref().unwrap().contains("loved"));
        assert!(reply.parts_json.as_deref().unwrap().contains("hello imessage"));
        assert_eq!(reply.owner_display_name.as_deref(), Some("+15555550100"));

        let tapback = &mail_messages[1];
        assert_eq!(tapback.associated_guid.as_deref(), Some("parent-guid-1111"));
        assert_eq!(tapback.associated_part, Some(0));
        assert_eq!(tapback.tapback_kind.as_deref(), Some("loved"));
        assert_eq!(tapback.tapback_action.as_deref(), Some("add"));
        // Owner display name falls back to `export.owner_display_name`.
        assert_eq!(tapback.owner_display_name.as_deref(), Some("Me"));
    }

    #[test]
    fn imessage_csv_uses_fork_compatible_headers() {
        let tmp = tempfile::tempdir().unwrap();
        let doc = sample_imessage_doc();

        let csv_path = write_format(tmp.path(), OutputFormat::Csv, &doc).unwrap();
        let csv = fs::read_to_string(&csv_path).unwrap();
        let header_line = csv.lines().next().unwrap();
        assert_eq!(header_line, IMESSAGE_CSV_HEADERS.join(","));
        assert!(csv.contains("hello imessage"));
        assert!(csv.contains("Loved a message"));
        assert!(csv.contains("Sent with Balloons"));
        assert!(csv.contains("true")); // is_reply
        assert!(csv.contains("loved"));

        // Non-iMessage sources keep the shared SBR-style headers.
        let sbr_doc = sample_doc();
        let sbr_csv_path = write_format(tmp.path(), OutputFormat::Csv, &sbr_doc).unwrap();
        let sbr_csv = fs::read_to_string(&sbr_csv_path).unwrap();
        assert_eq!(sbr_csv.lines().next().unwrap(), CSV_HEADERS.join(","));
    }
}
