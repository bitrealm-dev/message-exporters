//! Project message-ir v3 records into vault NDJSON v1 wire lines.

use anyhow::{Context, Result, bail};
use csv::format_local_ts;
use ir::{
    ConversationDocument, ConversationHeader, IrAttachment, IrDirection, IrImessage, IrMessage,
    IrService, SCHEMA_VERSION,
};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize)]
struct VaultConversation {
    record: &'static str,
    schema: &'static str,
    schema_version: u32,
    chat_identifier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    service: Option<String>,
    conversation_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    group_title: Option<String>,
    participants: Vec<VaultParticipant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    export_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    export_tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    export_tool_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct VaultParticipant {
    handle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct VaultAttachment {
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    original_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    is_sticker: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    transcription: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct VaultMessage {
    record: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    guid: Option<String>,
    timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp_utc: Option<String>,
    is_from_me: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    sender: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_receipt: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    is_deleted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    send_effect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shared_location: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    is_announcement: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    announcement: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<VaultAttachment>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tapbacks: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    parts: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    edits: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app: Option<Value>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    is_reply: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_originator_guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_originator_part: Option<i64>,
    #[serde(skip_serializing_if = "is_zero_i64")]
    num_replies: i64,
}

fn is_zero_i64(n: &i64) -> bool {
    *n == 0
}

pub fn validate_header(header: &ConversationHeader) -> Result<String> {
    if header.schema_version != SCHEMA_VERSION {
        bail!(
            "unsupported schema_version {} (expected {})",
            header.schema_version,
            SCHEMA_VERSION
        );
    }
    let source = header.export.source.trim();
    if source.is_empty() {
        bail!("export.source is empty");
    }
    Ok(source.to_string())
}

pub fn conversation_line(header: &ConversationHeader) -> Result<Vec<u8>> {
    let source = validate_header(header)?;
    let service = dominant_service_label(header);
    let rec = VaultConversation {
        record: "conversation",
        schema: "vault",
        schema_version: 1,
        chat_identifier: header.conversation.chat_identifier.clone(),
        service,
        conversation_type: header.conversation.conversation_type.as_str().to_string(),
        group_title: header.conversation.group_title.clone(),
        participants: header
            .conversation
            .participants
            .iter()
            .map(|p| VaultParticipant {
                handle: p.handle.clone(),
                name_hint: p.display_name.clone(),
            })
            .collect(),
        export_source: Some(source),
        export_tool: Some(header.export.tool.clone()),
        export_tool_version: Some(header.export.tool_version.clone()),
    };
    let mut out = serde_json::to_vec(&rec).context("serialize vault conversation")?;
    out.push(b'\n');
    Ok(out)
}

pub fn message_line(msg: &IrMessage, digests: &[(usize, String)]) -> Result<(Vec<u8>, String)> {
    let secs = msg.timestamp_unix_ms.div_euclid(1000);
    let (ts_local, ts_utc, _) = format_local_ts(secs).ok_or_else(|| {
        anyhow::anyhow!(
            "unrepresentable timestamp_unix_ms {}",
            msg.timestamp_unix_ms
        )
    })?;
    let is_from_me = msg.direction == IrDirection::Outgoing;
    let im = msg.imessage.as_ref();
    let attachments = project_attachments(&msg.attachments, digests);
    let guid = if msg.guid.trim().is_empty() {
        None
    } else {
        Some(msg.guid.clone())
    };
    let rec = VaultMessage {
        record: "message",
        guid: guid.clone(),
        timestamp: ts_local,
        timestamp_utc: Some(ts_utc),
        is_from_me,
        sender: if is_from_me {
            None
        } else {
            msg.sender_handle.clone()
        },
        service: Some(service_label(msg.service)),
        subject: msg.subject.clone().filter(|s| !s.is_empty()),
        text: {
            let t = msg.text.trim();
            if t.is_empty() {
                None
            } else {
                Some(msg.text.clone())
            }
        },
        read_receipt: im.and_then(|i| i.read_receipt_rfc3339.clone()),
        is_deleted: im.map(|i| i.is_deleted).unwrap_or(false),
        send_effect: im.and_then(|i| i.send_effect.clone()),
        shared_location: im.and_then(|i| i.shared_location.clone()),
        is_announcement: im.map(|i| i.announcement.is_some()).unwrap_or(false)
            || matches!(msg.message_kind, ir::IrMessageKind::Announcement),
        announcement: im.and_then(|i| i.announcement.clone()),
        attachments,
        tapbacks: value_array(im.and_then(|i| i.tapbacks.as_ref())),
        parts: value_array(im.and_then(|i| i.parts.as_ref())),
        edits: value_array(im.and_then(|i| i.edits.as_ref())),
        app: im.and_then(|i| i.app.clone()),
        is_reply: im.map(|i| i.is_reply).unwrap_or(false),
        thread_originator_guid: im.and_then(|i| i.in_reply_to_guid.clone()),
        thread_originator_part: im.and_then(|i| i.thread_originator_part.map(|p| i64::from(p))),
        num_replies: im
            .and_then(|i| i.num_replies.map(|n| i64::from(n)))
            .unwrap_or(0),
    };
    // Preserve standalone tapback rows that only set kind/emoji on the IR bag.
    let rec = enrich_tapback_fields(rec, im);
    let mut out = serde_json::to_vec(&rec).context("serialize vault message")?;
    out.push(b'\n');
    let guid = guid.unwrap_or_else(|| format!("unguided:{}", msg.timestamp_unix_ms));
    Ok((out, guid))
}

fn enrich_tapback_fields(mut rec: VaultMessage, im: Option<&IrImessage>) -> VaultMessage {
    let Some(im) = im else {
        return rec;
    };
    if !rec.tapbacks.is_empty() {
        return rec;
    }
    if let Some(kind) = im.tapback_kind.as_ref().filter(|s| !s.is_empty()) {
        rec.tapbacks.push(json!({
            "part_index": im.associated_part.unwrap_or(0),
            "kind": kind,
            "emoji": im.tapback_emoji,
            "is_from_me": rec.is_from_me,
            "sender": rec.sender,
        }));
    }
    rec
}

fn project_attachments(
    attachments: &[IrAttachment],
    digests: &[(usize, String)],
) -> Vec<VaultAttachment> {
    let digest_map: std::collections::HashMap<usize, &str> =
        digests.iter().map(|(i, d)| (*i, d.as_str())).collect();
    attachments
        .iter()
        .enumerate()
        .map(|(i, a)| VaultAttachment {
            path: a.path.clone(),
            original_name: a.original_name.clone(),
            mime_type: a.mime_type.clone(),
            sha256: digest_map
                .get(&i)
                .map(|s| (*s).to_string())
                .or_else(|| a.digest_sha256.clone()),
            is_sticker: a.is_sticker,
            transcription: a.transcription.clone(),
        })
        .collect()
}

fn value_array(v: Option<&Value>) -> Vec<Value> {
    match v {
        Some(Value::Array(items)) => items.clone(),
        Some(other) if !other.is_null() => vec![other.clone()],
        _ => Vec::new(),
    }
}

fn service_label(service: IrService) -> String {
    match service {
        IrService::Sms => "SMS".into(),
        IrService::IMessage => "iMessage".into(),
        IrService::Whatsapp => "WhatsApp".into(),
        IrService::Rcs => "RCS".into(),
        IrService::Unknown => "Unknown".into(),
    }
}

fn dominant_service_label(header: &ConversationHeader) -> Option<String> {
    // Header has no messages; leave None and let per-message service fill in.
    // Callers that have a document can pass a better guess via document_conversation_line.
    let _ = header;
    None
}

pub fn document_conversation_line(doc: &ConversationDocument) -> Result<Vec<u8>> {
    let header = ConversationHeader::from_document(doc);
    let mut line = conversation_line(&header)?;
    // Inject dominant service from first message when present.
    if let Some(msg) = doc.messages.first() {
        let mut v: Value = serde_json::from_slice(line.trim_ascii())?;
        if let Some(obj) = v.as_object_mut() {
            obj.insert("service".into(), json!(service_label(msg.service)));
        }
        line = serde_json::to_vec(&v)?;
        line.push(b'\n');
    }
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ir::{
        ConversationMeta, ConversationStats, ExportMeta, IrConversationType, IrMessageKind,
        IrParticipant,
    };

    #[test]
    fn projects_basic_sms() {
        let header = ConversationHeader {
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
        };
        let conv = String::from_utf8(conversation_line(&header).unwrap()).unwrap();
        assert!(conv.contains(r#""record":"conversation""#));
        assert!(conv.contains(r#""export_source":"sms-backup-restore""#));

        let msg = IrMessage {
            guid: "g1".into(),
            timestamp_unix_ms: 1_400_773_261_000,
            direction: IrDirection::Incoming,
            service: IrService::Sms,
            message_kind: IrMessageKind::Sms,
            sender_handle: Some("+15555550101".into()),
            sender_display_name: Some("Sam".into()),
            subject: None,
            text: "hello".into(),
            attachments: vec![],
            imessage: None,
            source: None,
        };
        let (line, guid) = message_line(&msg, &[]).unwrap();
        assert_eq!(guid, "g1");
        let s = String::from_utf8(line).unwrap();
        assert!(s.contains(r#""is_from_me":false"#));
        assert!(s.contains(r#""record":"message""#));
    }
}
