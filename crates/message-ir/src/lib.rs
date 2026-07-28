//! Canonical conversation intermediate representation (IR).
//!
//! Source exporters parse vendor formats into [`ConversationDocument`], then
//! project with [`write_format`] to CSV, EML, MBOX, JSON, or JSONL. See
//! [`docs/MESSAGE_IR.md`](../../../docs/MESSAGE_IR.md).
//!
//! Schema version 3 is a typed, stable JSON shape: enums for service/kind,
//! struct bags for `imessage` / `source`, filled outgoing identity, conversation
//! stats, and packaging stem suffixes kept out of serialized JSON.

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
use serde_json::{json, Map, Value};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 3;

/// Unified CSV columns for every exporter (IR v3 projection).
///
/// Apple-only cells are empty for non-iMessage sources. Legacy names
/// (`date_ms`, `contact_name`, `xml_fields_json`) are gone — use
/// `timestamp_unix_ms`, `sender_display_name`, and `source_fields_json`.
pub const CSV_HEADERS: &[&str] = &[
    "chat_identifier",
    "conversation_type",
    "group_title",
    "participants_json",
    "guid",
    "timestamp",
    "timestamp_utc",
    "timestamp_display",
    "timestamp_unix_ms",
    "direction",
    "service",
    "sender_handle",
    "sender_display_name",
    "subject",
    "text",
    "attachments_json",
    "message_kind",
    "export_source",
    "export_tool",
    "export_tool_version",
    "owner_handle",
    "owner_display_name",
    "android_type",
    "source_fields_json",
    "read_receipt",
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
    "tapbacks_json",
    "app_json",
];

/// Alias kept for callers that still name the Apple header set.
pub const IMESSAGE_CSV_HEADERS: &[&str] = CSV_HEADERS;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationDocument {
    pub schema_version: u32,
    pub export: ExportMeta,
    pub conversation: ConversationMeta,
    pub messages: Vec<IrMessage>,
    /// On-disk stem suffix (e.g. `__whatsapp`). Never serialized into JSON/JSONL.
    #[serde(skip)]
    pub packaging_stem_suffix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMeta {
    pub source: String,
    pub tool: String,
    pub tool_version: String,
    pub owner_handle: Option<String>,
    /// Outgoing display name; emitters should set when known (iMessage caller-id / `"Me"`).
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
    pub group_title: Option<String>,
    pub participants: Vec<IrParticipant>,
    pub stats: ConversationStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ConversationStats {
    pub message_count: usize,
    pub attachment_count: usize,
    pub first_timestamp_unix_ms: Option<i64>,
    pub last_timestamp_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrParticipant {
    pub handle: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IrService {
    Sms,
    #[serde(rename = "imessage")]
    IMessage,
    Whatsapp,
    Rcs,
    Unknown,
}

impl IrService {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sms => "sms",
            Self::IMessage => "imessage",
            Self::Whatsapp => "whatsapp",
            Self::Rcs => "rcs",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "sms" => Self::Sms,
            "imessage" => Self::IMessage,
            "whatsapp" => Self::Whatsapp,
            "rcs" => Self::Rcs,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IrMessageKind {
    Sms,
    Mms,
    #[serde(rename = "imessage")]
    IMessage,
    Tapback,
    StickerTapback,
    Announcement,
    LocationShare,
    Balloon,
    Unknown,
}

impl IrMessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sms => "sms",
            Self::Mms => "mms",
            Self::IMessage => "imessage",
            Self::Tapback => "tapback",
            Self::StickerTapback => "sticker_tapback",
            Self::Announcement => "announcement",
            Self::LocationShare => "location_share",
            Self::Balloon => "balloon",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "sms" => Self::Sms,
            "mms" => Self::Mms,
            "imessage" => Self::IMessage,
            "tapback" => Self::Tapback,
            "sticker_tapback" => Self::StickerTapback,
            "announcement" => Self::Announcement,
            "location_share" => Self::LocationShare,
            "balloon" => Self::Balloon,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrMessage {
    pub guid: String,
    pub timestamp_unix_ms: i64,
    pub direction: IrDirection,
    pub service: IrService,
    pub message_kind: IrMessageKind,
    pub sender_handle: Option<String>,
    pub sender_display_name: Option<String>,
    pub subject: Option<String>,
    pub text: String,
    pub attachments: Vec<IrAttachment>,
    pub imessage: Option<IrImessage>,
    pub source: Option<IrSource>,
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
    pub path: Option<String>,
    pub original_name: Option<String>,
    pub mime_type: Option<String>,
    pub digest_sha256: Option<String>,
    pub is_sticker: bool,
    pub transcription: Option<String>,
    pub sticker_effect: Option<String>,
    /// In-memory bytes for EML embedding; never written to JSON.
    #[serde(skip)]
    pub bytes: Option<Vec<u8>>,
}

/// Vendor leftovers. Display names live on `sender_display_name`, not here.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IrSource {
    pub android_type: Option<i32>,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

impl IrSource {
    pub fn is_empty(&self) -> bool {
        self.android_type.is_none() && self.fields.is_empty()
    }

    pub fn into_option(self) -> Option<Self> {
        if self.is_empty() {
            None
        } else {
            Some(self)
        }
    }
}

/// iMessage extensions. Nested Apple blobs remain JSON values (not strings).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IrImessage {
    pub is_reply: bool,
    pub in_reply_to_guid: Option<String>,
    pub thread_originator_part: Option<u32>,
    pub num_replies: Option<u32>,
    pub is_deleted: bool,
    pub send_effect: Option<String>,
    pub shared_location: Option<String>,
    pub announcement: Option<String>,
    pub read_receipt_rfc3339: Option<String>,
    pub parts: Option<Value>,
    pub edits: Option<Value>,
    pub tapbacks: Option<Value>,
    pub app: Option<Value>,
    pub balloon_bundle_id: Option<String>,
    pub balloon_kind: Option<String>,
    pub associated_guid: Option<String>,
    pub associated_part: Option<u32>,
    pub tapback_kind: Option<String>,
    pub tapback_emoji: Option<String>,
    pub tapback_action: Option<String>,
}

impl IrImessage {
    pub fn is_empty(&self) -> bool {
        !self.is_reply
            && self.in_reply_to_guid.is_none()
            && self.thread_originator_part.is_none()
            && self.num_replies.is_none()
            && !self.is_deleted
            && self.send_effect.is_none()
            && self.shared_location.is_none()
            && self.announcement.is_none()
            && self.read_receipt_rfc3339.is_none()
            && self.parts.is_none()
            && self.edits.is_none()
            && self.tapbacks.is_none()
            && self.app.is_none()
            && self.balloon_bundle_id.is_none()
            && self.balloon_kind.is_none()
            && self.associated_guid.is_none()
            && self.associated_part.is_none()
            && self.tapback_kind.is_none()
            && self.tapback_emoji.is_none()
            && self.tapback_action.is_none()
    }

    pub fn into_option(self) -> Option<Self> {
        if self.is_empty() {
            None
        } else {
            Some(self)
        }
    }
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
            self.packaging_stem_suffix.as_deref(),
        );
        csv.strip_suffix(".csv")
            .unwrap_or(csv.as_str())
            .to_string()
    }

    /// Recompute [`ConversationMeta::stats`] from `messages`.
    pub fn finalize_stats(&mut self) {
        self.conversation.stats = compute_stats(&self.messages);
    }
}

fn compute_stats(messages: &[IrMessage]) -> ConversationStats {
    let message_count = messages.len();
    let attachment_count = messages.iter().map(|m| m.attachments.len()).sum();
    let mut first = None;
    let mut last = None;
    for msg in messages {
        first = Some(first.map_or(msg.timestamp_unix_ms, |f: i64| f.min(msg.timestamp_unix_ms)));
        last = Some(last.map_or(msg.timestamp_unix_ms, |l: i64| l.max(msg.timestamp_unix_ms)));
    }
    ConversationStats {
        message_count,
        attachment_count,
        first_timestamp_unix_ms: first,
        last_timestamp_unix_ms: last,
    }
}

/// Owner identity for outgoing rows: handle + display (`"Me"` if handle set but name missing).
pub fn owner_sender(export: &ExportMeta) -> (Option<String>, Option<String>) {
    let handle = export
        .owner_handle
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let display = export
        .owner_display_name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| handle.as_ref().map(|_| "Me".into()));
    (handle, display)
}

/// Parse Android type strings / numbers into `i32`.
pub fn parse_android_type(s: &str) -> Option<i32> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.parse::<i32>().ok()
}

/// Parse a JSON string into a [`Value`], or return the string as a JSON string value.
pub fn parse_json_value(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|_| json!(s))
}

/// Write one conversation in the requested packaging format.
pub fn write_format(
    output_dir: &Path,
    format: OutputFormat,
    doc: &ConversationDocument,
) -> Result<PathBuf> {
    let mut doc = doc.clone();
    doc.finalize_stats();
    match format {
        OutputFormat::Csv => write_conversation_csv(output_dir, &doc),
        OutputFormat::Json => write_conversation_json(output_dir, &doc),
        OutputFormat::Jsonl => write_conversation_jsonl(output_dir, &doc),
        OutputFormat::Eml => write_conversation_mail(output_dir, &doc, MailPackage::EmlFolders),
        OutputFormat::Mbox => write_conversation_mail(output_dir, &doc, MailPackage::Mbox),
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

fn stem_suffix(doc: &ConversationDocument) -> Option<&str> {
    doc.packaging_stem_suffix.as_deref()
}

fn value_cell(v: Option<&Value>) -> String {
    v.map(|v| serde_json::to_string(v).unwrap_or_else(|_| "null".into()))
        .unwrap_or_else(|| "null".into())
}

fn value_as_string(v: Option<&Value>) -> Option<String> {
    let v = v?;
    if v.is_null() {
        return None;
    }
    Some(serde_json::to_string(v).unwrap_or_default()).filter(|s| !s.is_empty())
}

#[derive(Serialize)]
struct ParticipantCell {
    handle: String,
    display_name: String,
}

/// Per-conversation CSV using the unified [`CSV_HEADERS`] contract.
pub fn write_conversation_csv(output_dir: &Path, doc: &ConversationDocument) -> Result<PathBuf> {
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
        stem_suffix(doc),
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

    let participants_json = json_cell(
        &doc.conversation
            .participants
            .iter()
            .map(|p| ParticipantCell {
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
        let attachment_cells: Vec<AttachmentCell> = msg
            .attachments
            .iter()
            .map(|a| AttachmentCell {
                path: a.path.clone(),
                original_name: a.original_name.clone(),
                mime_type: a.mime_type.clone(),
                digest_sha256: a.digest_sha256.clone(),
                is_sticker: a.is_sticker,
                transcription: a.transcription.clone(),
                sticker_effect: a.sticker_effect.clone(),
            })
            .collect();
        let attachments_json = json_cell(&attachment_cells);
        let timestamp_unix_ms = msg.timestamp_unix_ms.to_string();
        let android_type = msg
            .source
            .as_ref()
            .and_then(|s| s.android_type)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let source_fields_json = msg
            .source
            .as_ref()
            .filter(|s| !s.fields.is_empty())
            .map(|s| serde_json::to_string(&s.fields).unwrap_or_default())
            .unwrap_or_default();

        let im = msg.imessage.as_ref();
        let read_receipt = im
            .and_then(|i| i.read_receipt_rfc3339.as_deref())
            .unwrap_or("");
        let is_deleted = im.map(|i| i.is_deleted).unwrap_or(false);
        let send_effect = im.and_then(|i| i.send_effect.as_deref()).unwrap_or("");
        let shared_location = im.and_then(|i| i.shared_location.as_deref()).unwrap_or("");
        let is_announcement = msg.message_kind == IrMessageKind::Announcement;
        let announcement = im.and_then(|i| i.announcement.as_deref()).unwrap_or("");
        let is_reply = im.map(|i| i.is_reply).unwrap_or(false);
        let thread_originator_guid = if is_reply {
            im.and_then(|i| i.in_reply_to_guid.as_deref()).unwrap_or("")
        } else {
            ""
        };
        let thread_originator_part = if is_reply {
            im.and_then(|i| i.thread_originator_part)
                .unwrap_or(0)
                .to_string()
        } else {
            String::new()
        };
        let num_replies = im
            .and_then(|i| i.num_replies)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let parts_json = value_cell(im.and_then(|i| i.parts.as_ref()));
        let edits_json = value_cell(im.and_then(|i| i.edits.as_ref()));
        let tapbacks_json = value_cell(im.and_then(|i| i.tapbacks.as_ref()));
        let app_json = value_cell(im.and_then(|i| i.app.as_ref()));

        wtr.write_record([
            doc.conversation.chat_identifier.as_str(),
            doc.conversation.conversation_type.as_str(),
            doc.conversation.group_title.as_deref().unwrap_or(""),
            participants_json.as_str(),
            msg.guid.as_str(),
            ts_local.as_str(),
            ts_utc.as_str(),
            ts_display.as_str(),
            timestamp_unix_ms.as_str(),
            msg.direction.as_str(),
            msg.service.as_str(),
            msg.sender_handle.as_deref().unwrap_or(""),
            msg.sender_display_name.as_deref().unwrap_or(""),
            msg.subject.as_deref().unwrap_or(""),
            msg.text.as_str(),
            attachments_json.as_str(),
            msg.message_kind.as_str(),
            doc.export.source.as_str(),
            doc.export.tool.as_str(),
            doc.export.tool_version.as_str(),
            doc.export.owner_handle.as_deref().unwrap_or(""),
            doc.export.owner_display_name.as_deref().unwrap_or(""),
            android_type.as_str(),
            source_fields_json.as_str(),
            read_receipt,
            if is_deleted { "true" } else { "false" },
            send_effect,
            shared_location,
            if is_announcement { "true" } else { "false" },
            announcement,
            if is_reply { "true" } else { "false" },
            thread_originator_guid,
            thread_originator_part.as_str(),
            num_replies.as_str(),
            parts_json.as_str(),
            edits_json.as_str(),
            tapbacks_json.as_str(),
            app_json.as_str(),
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

        let android_type = msg
            .source
            .as_ref()
            .and_then(|s| s.android_type)
            .map(|n| n.to_string());
        let source_fields_json = msg.source.as_ref().and_then(|s| {
            if s.fields.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&s.fields).unwrap_or_default())
            }
        });

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
            service: msg.service.as_str().to_string(),
            message_kind: msg.message_kind.as_str().to_string(),
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
            filename_suffix: doc.packaging_stem_suffix.clone(),
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

/// Restore iMessage extension fields from [`IrImessage`] onto a [`MailMessage`].
pub fn apply_imessage_fields(mail: &mut MailMessage, imessage: &IrImessage) {
    mail.is_reply = imessage.is_reply;
    mail.in_reply_to_guid = imessage.in_reply_to_guid.clone();
    mail.thread_originator_part = imessage.thread_originator_part;
    mail.num_replies = imessage.num_replies;
    mail.is_deleted = imessage.is_deleted;
    mail.send_effect = imessage.send_effect.clone();
    mail.shared_location = imessage.shared_location.clone();
    mail.announcement = imessage.announcement.clone();
    mail.read_receipt_rfc3339 = imessage.read_receipt_rfc3339.clone();
    mail.parts_json = value_as_string(imessage.parts.as_ref());
    mail.edits_json = value_as_string(imessage.edits.as_ref());
    mail.app_json = value_as_string(imessage.app.as_ref());
    mail.balloon_bundle_id = imessage.balloon_bundle_id.clone();
    mail.balloon_kind = imessage.balloon_kind.clone();
    mail.tapbacks_json = value_as_string(imessage.tapbacks.as_ref());
    mail.associated_guid = imessage.associated_guid.clone();
    mail.associated_part = imessage.associated_part;
    mail.tapback_kind = imessage.tapback_kind.clone();
    mail.tapback_emoji = imessage.tapback_emoji.clone();
    mail.tapback_action = imessage.tapback_action.clone();
}

#[cfg(test)]
mod tests {
    use super::*;
    use message_mail::clean_previous_mail_output;

    fn sample_doc() -> ConversationDocument {
        let mut doc = ConversationDocument {
            schema_version: SCHEMA_VERSION,
            export: ExportMeta {
                source: "sms-backup-restore".into(),
                tool: "SMS Backup & Restore".into(),
                tool_version: "10.26.003".into(),
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
                stats: ConversationStats::default(),
            },
            messages: vec![
                IrMessage {
                    guid: "aabbccddeeff00112233445566778899".into(),
                    timestamp_unix_ms: 1_400_773_261_000,
                    direction: IrDirection::Incoming,
                    service: IrService::Sms,
                    message_kind: IrMessageKind::Sms,
                    sender_handle: Some("+15555550101".into()),
                    sender_display_name: Some("Sam".into()),
                    subject: None,
                    text: "hello ir".into(),
                    attachments: vec![],
                    imessage: None,
                    source: Some(IrSource {
                        android_type: Some(1),
                        fields: {
                            let mut m = Map::new();
                            m.insert("address".into(), json!("+15555550101"));
                            m
                        },
                    }),
                },
                IrMessage {
                    guid: "bbccddeeff00112233445566778899aa".into(),
                    timestamp_unix_ms: 1_400_773_262_000,
                    direction: IrDirection::Outgoing,
                    service: IrService::Sms,
                    message_kind: IrMessageKind::Sms,
                    sender_handle: Some("+15555550100".into()),
                    sender_display_name: Some("Me".into()),
                    subject: None,
                    text: "outgoing".into(),
                    attachments: vec![],
                    imessage: None,
                    source: Some(IrSource {
                        android_type: Some(2),
                        fields: Map::new(),
                    }),
                },
            ],
            packaging_stem_suffix: None,
        };
        doc.finalize_stats();
        doc
    }

    #[test]
    fn writes_json_csv_jsonl_and_eml() {
        let tmp = tempfile::tempdir().unwrap();
        let doc = sample_doc();

        let json_path = write_format(tmp.path(), OutputFormat::Json, &doc).unwrap();
        assert!(json_path.ends_with("+15555550101.json"));
        let raw = fs::read_to_string(&json_path).unwrap();
        let parsed: ConversationDocument = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.schema_version, 3);
        assert_eq!(parsed.messages[0].text, "hello ir");
        assert!(parsed.messages[0].attachments.is_empty());
        assert_eq!(parsed.messages[0].source.as_ref().unwrap().android_type, Some(1));
        assert!(parsed.messages[0].source.as_ref().unwrap().fields.contains_key("address"));
        assert_eq!(
            parsed.messages[1].sender_handle.as_deref(),
            Some("+15555550100")
        );
        assert_eq!(parsed.conversation.stats.message_count, 2);
        assert!(!raw.contains("filename_suffix"));
        assert!(!raw.contains("\"bytes\""));
        // Stable null keys present.
        assert!(raw.contains("\"group_title\": null") || raw.contains("\"group_title\":null"));
        assert!(raw.contains("\"imessage\": null") || raw.contains("\"imessage\":null"));

        let jsonl_path = write_format(tmp.path(), OutputFormat::Jsonl, &doc).unwrap();
        assert!(jsonl_path.ends_with("+15555550101.jsonl"));
        let jsonl = fs::read_to_string(&jsonl_path).unwrap();
        let mut lines = jsonl.lines();
        let header: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(header["schema_version"], 3);
        assert!(header.get("messages").is_none());
        assert_eq!(header["conversation"]["stats"]["message_count"], 2);
        let msg_line: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(msg_line["text"], "hello ir");
        assert!(msg_line["source"]["fields"].is_object());
        assert_eq!(msg_line["source"]["android_type"], 1);

        let csv_path = write_format(tmp.path(), OutputFormat::Csv, &doc).unwrap();
        let csv = fs::read_to_string(&csv_path).unwrap();
        assert!(csv.contains("hello ir"));
        assert!(csv.contains("sms-backup-restore"));
        assert!(csv.contains("source_fields_json"));
        assert!(csv.contains("timestamp_unix_ms"));
        assert!(csv.contains("+15555550100")); // outgoing sender filled

        let _ = clean_previous_mail_output(tmp.path());
        let eml_dir = write_format(tmp.path(), OutputFormat::Eml, &doc).unwrap();
        assert!(eml_dir.is_dir());
    }

    fn sample_imessage_doc() -> ConversationDocument {
        let mut doc = ConversationDocument {
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
                stats: ConversationStats::default(),
            },
            messages: vec![
                IrMessage {
                    guid: "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE".into(),
                    timestamp_unix_ms: 1_400_773_261_000,
                    direction: IrDirection::Incoming,
                    service: IrService::IMessage,
                    message_kind: IrMessageKind::IMessage,
                    sender_handle: Some("+15555550101".into()),
                    sender_display_name: Some("Sam".into()),
                    subject: None,
                    text: "hello imessage".into(),
                    attachments: vec![],
                    imessage: Some(IrImessage {
                        is_reply: true,
                        in_reply_to_guid: Some("parent-guid-1111".into()),
                        thread_originator_part: Some(0),
                        num_replies: Some(2),
                        send_effect: Some("Sent with Balloons".into()),
                        tapbacks: Some(json!([{"part_index": 0, "kind": "loved"}])),
                        parts: Some(json!([{"index": 0, "kind": "run", "text": "hello imessage"}])),
                        ..IrImessage::default()
                    }),
                    source: None,
                },
                IrMessage {
                    guid: "TAPBACK-GUID-0001".into(),
                    timestamp_unix_ms: 1_400_773_262_000,
                    direction: IrDirection::Outgoing,
                    service: IrService::IMessage,
                    message_kind: IrMessageKind::Tapback,
                    sender_handle: Some("+15555550100".into()),
                    sender_display_name: Some("Me".into()),
                    subject: None,
                    text: "Loved a message".into(),
                    attachments: vec![],
                    imessage: Some(IrImessage {
                        associated_guid: Some("parent-guid-1111".into()),
                        associated_part: Some(0),
                        tapback_kind: Some("loved".into()),
                        tapback_action: Some("add".into()),
                        in_reply_to_guid: Some("parent-guid-1111".into()),
                        ..IrImessage::default()
                    }),
                    source: None,
                },
            ],
            packaging_stem_suffix: None,
        };
        doc.finalize_stats();
        doc
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
        assert_eq!(reply.owner_display_name.as_deref(), Some("Me"));

        let tapback = &mail_messages[1];
        assert_eq!(tapback.associated_guid.as_deref(), Some("parent-guid-1111"));
        assert_eq!(tapback.associated_part, Some(0));
        assert_eq!(tapback.tapback_kind.as_deref(), Some("loved"));
        assert_eq!(tapback.tapback_action.as_deref(), Some("add"));
        assert_eq!(tapback.owner_display_name.as_deref(), Some("Me"));
        assert_eq!(tapback.sender_handle.as_deref(), Some("+15555550100"));
    }

    #[test]
    fn unified_csv_headers_for_all_sources() {
        let tmp = tempfile::tempdir().unwrap();
        let doc = sample_imessage_doc();

        let csv_path = write_format(tmp.path(), OutputFormat::Csv, &doc).unwrap();
        let csv = fs::read_to_string(&csv_path).unwrap();
        let header_line = csv.lines().next().unwrap();
        assert_eq!(header_line, CSV_HEADERS.join(","));
        assert!(csv.contains("timestamp_unix_ms"));
        assert!(csv.contains("source_fields_json"));
        assert!(csv.contains("owner_handle"));
        assert!(!header_line.contains("date_ms"));
        assert!(!header_line.contains("contact_name"));
        assert!(!header_line.contains("xml_fields_json"));
        assert!(csv.contains("hello imessage"));
        assert!(csv.contains("Loved a message"));
        assert!(csv.contains("Sent with Balloons"));
        assert!(csv.contains("true")); // is_reply
        assert!(csv.contains("loved"));
        assert!(csv.contains("+15555550100")); // outgoing sender / owner

        let sbr_doc = sample_doc();
        let sbr_csv_path = write_format(tmp.path(), OutputFormat::Csv, &sbr_doc).unwrap();
        let sbr_csv = fs::read_to_string(&sbr_csv_path).unwrap();
        assert_eq!(sbr_csv.lines().next().unwrap(), CSV_HEADERS.join(","));
        assert!(sbr_csv.contains("xml_fields_json") == false);
        assert!(sbr_csv.contains("source_fields_json"));
    }

    #[test]
    fn packaging_stem_suffix_affects_filename_not_json() {
        let mut doc = sample_doc();
        doc.packaging_stem_suffix = Some("__whatsapp".into());
        let tmp = tempfile::tempdir().unwrap();
        let path = write_format(tmp.path(), OutputFormat::Json, &doc).unwrap();
        assert!(path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("__whatsapp"));
        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("filename_suffix"));
        assert!(!raw.contains("__whatsapp"));
    }
}
