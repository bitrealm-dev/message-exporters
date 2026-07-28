//! Canonical conversation intermediate representation (IR).
//!
//! Source exporters parse vendor formats into [`ConversationDocument`], then
//! project with [`write_format`] to CSV, EML, MBOX, or JSON. See
//! [`docs/MESSAGE_IR.md`](../../../docs/MESSAGE_IR.md).

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
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMeta {
    pub source: String,
    pub tool: String,
    pub tool_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_handle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub chat_identifier: String,
    pub conversation_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<IrParticipant>,
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
    /// iMessage extensions (parts, tapbacks, …). Omitted for SMS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imessage: Option<Value>,
    /// Vendor-specific bag (android_type, xml_fields, contact_name, date_ms, …).
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
            &self.conversation.conversation_type,
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

/// Per-conversation CSV (shared core + SBR-style source columns from `source`).
pub fn write_conversation_csv(output_dir: &Path, doc: &ConversationDocument) -> Result<PathBuf> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("create {}", output_dir.display()))?;
    let filename = conversation_filename(
        &doc.conversation.conversation_type,
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
        let date_ms = source_string(source, "date_ms")
            .unwrap_or_else(|| msg.timestamp_unix_ms.to_string());
        let contact_name = source_string(source, "contact_name").unwrap_or_default();
        let android_type = source_string(source, "android_type").unwrap_or_default();
        let xml_fields_json = source_string(source, "xml_fields_json")
            .or_else(|| {
                source
                    .and_then(|v| v.get("xml_fields"))
                    .map(|v| serde_json::to_string(v).unwrap_or_default())
            })
            .unwrap_or_default();

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
        let source_fields_json = source_string(msg.source.as_ref(), "xml_fields_json").or_else(
            || {
                msg.source
                    .as_ref()
                    .and_then(|v| v.get("xml_fields"))
                    .map(|v| serde_json::to_string(v).unwrap_or_default())
                    .filter(|s| !s.is_empty())
            },
        );

        out.push(MailMessage::sms(SmsMailFields {
            chat_identifier: doc.conversation.chat_identifier.clone(),
            conversation_type: doc.conversation.conversation_type.clone(),
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
        }));
    }
    Ok(out)
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
            },
            conversation: ConversationMeta {
                chat_identifier: "+15555550101".into(),
                conversation_type: "individual".into(),
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
                service: "SMS".into(),
                message_kind: "sms".into(),
                sender_handle: Some("+15555550101".into()),
                sender_display_name: Some("Sam".into()),
                subject: None,
                text: "hello ir".into(),
                attachments: vec![],
                imessage: None,
                source: Some(serde_json::json!({
                    "date_ms": "1400773261000",
                    "contact_name": "Sam",
                    "android_type": "1",
                    "xml_fields_json": "{\"address\":\"+15555550101\"}"
                })),
            }],
        }
    }

    #[test]
    fn writes_json_csv_and_eml() {
        let tmp = tempfile::tempdir().unwrap();
        let doc = sample_doc();

        let json_path = write_format(tmp.path(), OutputFormat::Json, &doc).unwrap();
        assert!(json_path.ends_with("+15555550101.json"));
        let parsed: ConversationDocument =
            serde_json::from_slice(&fs::read(&json_path).unwrap()).unwrap();
        assert_eq!(parsed.messages[0].text, "hello ir");
        assert!(parsed.messages[0].attachments.is_empty());

        let csv_path = write_format(tmp.path(), OutputFormat::Csv, &doc).unwrap();
        let csv = fs::read_to_string(&csv_path).unwrap();
        assert!(csv.contains("hello ir"));
        assert!(csv.contains("sms-backup-restore"));
        assert!(csv.contains("xml_fields_json"));

        let _ = clean_previous_mail_output(tmp.path());
        let eml_dir = write_format(tmp.path(), OutputFormat::Eml, &doc).unwrap();
        assert!(eml_dir.is_dir());
    }
}
