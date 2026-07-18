//! Convert GO SMS Pro export → per-conversation CSV.

use crate::owner_set::OwnerPhoneSet;
use crate::pdu::{parse_pdu_file, ParsedPdu};
use crate::phone::to_e164;
use crate::xml::{parse_xml_file, XmlMessage};
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
    pub xml_messages: u64,
    pub pdu_messages: u64,
    pub pdu_group_messages: u64,
    pub attachments_saved: u64,
    pub sent: u64,
    pub received: u64,
    pub skipped_invalid_date: u64,
    pub skipped_unknown_type: u64,
    pub skipped_unparseable_pdu: u64,
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

#[derive(Debug, Serialize)]
struct AttachmentCell {
    path: Option<String>,
    original_name: Option<String>,
    mime_type: Option<String>,
    is_sticker: bool,
    transcription: Option<String>,
    sticker_effect: Option<String>,
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

fn format_local_ts(secs: i64) -> (String, String, String) {
    let local = Local
        .timestamp_opt(secs, 0)
        .single()
        .unwrap_or_else(|| {
            Local.from_utc_datetime(
                &Utc.timestamp_opt(secs, 0)
                    .single()
                    .unwrap()
                    .naive_utc(),
            )
        });
    let utc = local.with_timezone(&Utc);
    let display = local.format("%b %e, %Y %I:%M:%S %p").to_string();
    (
        local.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        display,
    )
}

/// Deterministic message GUID from chat + timestamp + direction + body + attachment digests.
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

fn chat_id_individual(digits: &str) -> String {
    to_e164(digits)
}

fn chat_id_group(participant_digits: &[String], owners: &OwnerPhoneSet) -> (String, String) {
    let mut others: Vec<String> = participant_digits
        .iter()
        .filter(|d| !owners.is_owner(d) && *d != "Unknown")
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
        let name = format!(
            "{}-I_{}_{}{}",
            date_prefix,
            parsed.timestamp,
            idx + 1,
            att.ext
        );
        let path = attachments_dir.join(&name);
        if !path.exists() {
            fs::write(&path, &att.data)?;
            report.attachments_saved += 1;
        }
        let digest_hex = hex::encode(Sha256::digest(&att.data));
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
    report.pdu_messages += 1;
    if parsed.is_sent {
        report.sent += 1;
    } else {
        report.received += 1;
    }

    let targets: Vec<(String, String, Option<String>)> = if parsed.is_group {
        report.pdu_group_messages += 1;
        let (id, title) = chat_id_group(&parsed.participants, owners);
        vec![(id, "group".to_string(), Some(title))]
    } else {
        let others: Vec<_> = parsed
            .participants
            .iter()
            .filter(|p| !owners.is_owner(p))
            .cloned()
            .collect();
        if others.is_empty() {
            return;
        }
        let other = &others[0];
        vec![(
            chat_id_individual(other),
            "individual".to_string(),
            None,
        )]
    };

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

    let pending = PendingMessage {
        sort_key: parsed.timestamp as f64,
        is_from_me: parsed.is_sent,
        sender_digits: if parsed.is_sent {
            None
        } else {
            Some(parsed.sender_number.clone())
        },
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

fn write_conversation(
    output_dir: &Path,
    chat_id: &str,
    convo: &mut PendingConversation,
) -> Result<()> {
    dedupe_messages(&mut convo.messages);
    if convo.messages.is_empty() {
        return Ok(());
    }

    let path = output_dir.join(safe_filename(chat_id));
    let file = File::create(&path).with_context(|| format!("create {}", path.display()))?;
    let mut wtr = csv::Writer::from_writer(file);
    wtr.write_record(HEADERS)
        .with_context(|| format!("write header {}", path.display()))?;

    for msg in &convo.messages {
        let secs = msg.sort_key as i64;
        let (ts_local, ts_utc, ts_display) = format_local_ts(secs);
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
    }

    wtr.flush()?;
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
    let owner = owners.primary_digits.clone();
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    // Clean previous CSV / leftover NDJSON (keep attachments if re-run; rewrite as needed).
    fs::create_dir_all(output_dir)?;
    for entry in fs::read_dir(output_dir)? {
        let entry = entry?;
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());
        if matches!(ext, Some("csv") | Some("json")) {
            let _ = fs::remove_file(&path);
        }
    }
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
        match parse_xml_file(&xml_path, &owner) {
            Ok((msgs, stats)) => {
                report.xml_messages += stats.messages;
                report.skipped_invalid_date += stats.skipped_invalid_date;
                report.skipped_unknown_type += stats.skipped_unknown_type;
                report.sent += stats.sent;
                report.received += stats.received;
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
        write_conversation(output_dir, &chat_id, &mut convo)?;
        report.conversations += 1;
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn convert_smoke_writes_csv_not_json() {
        let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_export");
        let output = tempfile_dir();
        let report = convert_export(&input, &output, &["+15555550100".into()]).unwrap();
        assert!(report.conversations >= 1);
        assert!(report.xml_messages >= 2);

        let mut csv_files: Vec<_> = fs::read_dir(&output)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("csv"))
            .collect();
        csv_files.sort();
        assert!(!csv_files.is_empty(), "expected at least one .csv");

        let json_count = fs::read_dir(&output)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
            .count();
        assert_eq!(json_count, 0);

        let mut contents = String::new();
        File::open(&csv_files[0])
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        let header = contents.lines().next().unwrap();
        assert!(header.contains("chat_identifier"));
        assert!(header.contains("direction"));
        assert!(header.contains("export_source"));
        assert!(header.contains("source_kind"));
        assert!(header.contains("xml_fields_json"));
        assert!(!header.contains("participants_json"));
        assert!(!header.contains("read_receipt"));
        assert!(!header.contains("tapbacks_json"));
        assert!(contents.contains("go-sms-pro"));
        assert!(contents.contains("incoming") || contents.contains("outgoing"));
        assert!(contents.contains("smoke"));
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "go-sms-pro-exporter-csv-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
