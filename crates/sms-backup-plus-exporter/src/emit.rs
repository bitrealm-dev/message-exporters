//! `convert`: turn SMS Backup+ `.eml` trees into SMS NDJSON (`message_json::sms`).

use crate::archive::parse_archive_eml_mail;
use crate::contacts::{ContactsBook, NameMapping, apply_name_mapping, fill_unknown_phone};
use crate::flat_eml::{is_archive_eml, is_flat_sms_eml, parse_flat_eml_mail};
use crate::identity::{chat_id_for, local_datetime_from_secs, message_identity, safe_stem};
use crate::phone::{owner_digits, sanitize_number, to_e164};
use crate::types::{AttachmentBlob, ParsedMessage};
use anyhow::{Context, Result, bail};
use chrono::{Local, Utc};
use message_json::sms::{
    AttachmentRecord, ConversationRecord, ExportRecord, MessageRecord, ParticipantRecord,
    stable_guid,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct ExportReport {
    pub conversations: u64,
    pub flat_eml: u64,
    pub archive_eml: u64,
    pub messages: u64,
    pub attachments_saved: u64,
    pub sent: u64,
    pub received: u64,
    /// `.eml` files that are not SMS Backup+ shaped (or SMS with unknown chat).
    pub skipped_not_sms_backup_plus: u64,
    pub skipped_invalid_date: u64,
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
    text: String,
    attachments: Vec<PendingAttachment>,
    dedupe_key: String,
}

#[derive(Debug, Default)]
struct PendingConversation {
    conversation_type: String,
    group_title: Option<String>,
    participants: BTreeMap<String, Option<String>>,
    messages: Vec<PendingMessage>,
}

fn format_local_ts(secs: i64) -> (String, String) {
    let local = local_datetime_from_secs(secs);
    let utc = local.with_timezone(&Utc);
    (
        local.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    )
}

fn chat_id_for_msg(msg: &ParsedMessage) -> String {
    chat_id_for(msg)
}

fn safe_filename(chat_id: &str) -> String {
    format!("{}.json", safe_stem(chat_id))
}

fn write_attachments(
    blobs: &[AttachmentBlob],
    attachments_dir: &Path,
    report: &mut ExportReport,
) -> Result<Vec<PendingAttachment>> {
    fs::create_dir_all(attachments_dir)?;
    let mut out = Vec::new();
    for blob in blobs {
        let path = attachments_dir.join(&blob.filename);
        if !path.exists() {
            fs::write(&path, &blob.data)?;
            report.attachments_saved += 1;
        }
        out.push(PendingAttachment {
            rel_path: format!("attachments/{}", blob.filename),
            original_name: blob.original_name.clone(),
            mime_type: blob.mime_type.clone(),
            digest_hex: hex::encode(Sha256::digest(&blob.data)),
        });
    }
    Ok(out)
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
            participants: BTreeMap::new(),
            messages: Vec::new(),
        })
}

fn add_message(
    conversations: &mut BTreeMap<String, PendingConversation>,
    msg: ParsedMessage,
    pending_atts: Vec<PendingAttachment>,
    report: &mut ExportReport,
) {
    let chat_id = chat_id_for_msg(&msg);
    let convo = ensure_convo(
        conversations,
        &chat_id,
        &msg.conversation_type,
        msg.group_title.clone(),
    );
    for (digits, hint) in &msg.participant_digits {
        let entry = convo.participants.entry(digits.clone()).or_insert(None);
        if entry.is_none() {
            *entry = hint.clone().or_else(|| msg.name_hint.clone());
        }
    }
    if msg.is_from_me {
        report.sent += 1;
    } else {
        report.received += 1;
    }
    report.messages += 1;
    let dedupe_key = message_identity(&msg);
    convo.messages.push(PendingMessage {
        sort_key: msg.timestamp_secs,
        is_from_me: msg.is_from_me,
        sender_digits: msg.sender_digits,
        text: msg.text,
        attachments: pending_atts,
        dedupe_key,
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
    exported_at: &str,
    owner_e164: &str,
) -> Result<()> {
    dedupe_messages(&mut convo.messages);
    if convo.messages.is_empty() {
        return Ok(());
    }

    let mut participants: Vec<ParticipantRecord> = convo
        .participants
        .iter()
        .map(|(digits, hint)| ParticipantRecord {
            handle: to_e164(digits),
            name_hint: hint.clone(),
        })
        .collect();
    if convo.conversation_type == "group"
        && !participants
            .iter()
            .any(|p| sanitize_number(&p.handle) == sanitize_number(owner_e164))
    {
        participants.push(ParticipantRecord {
            handle: owner_e164.to_string(),
            name_hint: None,
        });
    }
    participants.sort_by(|a, b| a.handle.cmp(&b.handle));

    let header = ConversationRecord::header(
        chat_id,
        convo.conversation_type.clone(),
        convo.group_title.clone(),
        participants,
        exported_at,
    );

    let path = output_dir.join(safe_filename(chat_id));
    let mut file = File::create(&path).with_context(|| format!("create {}", path.display()))?;
    serde_json::to_writer(&mut file, &ExportRecord::Conversation(header))?;
    file.write_all(b"\n")?;

    for msg in &convo.messages {
        let secs = msg.sort_key as i64;
        let (ts_local, ts_utc) = format_local_ts(secs);
        let digests: Vec<String> = msg
            .attachments
            .iter()
            .map(|a| a.digest_hex.clone())
            .collect();
        let guid = stable_guid(chat_id, &ts_local, msg.is_from_me, &msg.text, &digests);
        let sender = if msg.is_from_me {
            None
        } else {
            msg.sender_digits.as_ref().map(|d| to_e164(d))
        };
        let text = if msg.text.is_empty() {
            None
        } else {
            Some(msg.text.clone())
        };
        let attachments: Vec<AttachmentRecord> = msg
            .attachments
            .iter()
            .map(|a| AttachmentRecord {
                path: Some(a.rel_path.clone()),
                original_name: a.original_name.clone(),
                mime_type: a.mime_type.clone(),
            })
            .collect();

        let record = MessageRecord::text_message(
            guid,
            ts_local,
            Some(ts_utc),
            msg.is_from_me,
            sender,
            text,
            attachments,
        );
        serde_json::to_writer(&mut file, &ExportRecord::Message(record))?;
        file.write_all(b"\n")?;
    }
    Ok(())
}

pub(crate) fn collect_eml_paths<P: AsRef<Path>>(inputs: &[P]) -> Result<Vec<PathBuf>> {
    if inputs.is_empty() {
        bail!("at least one --input path is required");
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                // Skip obvious non-message trees
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(name.as_str(), "duplicate" | "exclude" | ".git") {
                    continue;
                }
                walk(&path, out)?;
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("eml"))
            {
                out.push(path);
            }
        }
        Ok(())
    }

    let mut paths = Vec::new();
    for input in inputs {
        let input = input.as_ref();
        if input.is_file() {
            if input
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("eml"))
            {
                paths.push(input.to_path_buf());
            } else {
                bail!("input file is not .eml: {}", input.display());
            }
            continue;
        }
        if !input.is_dir() {
            bail!("input is not a file or directory: {}", input.display());
        }
        walk(input, &mut paths)?;
    }

    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        let listed = inputs
            .iter()
            .map(|p| p.as_ref().display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        bail!("no .eml files under: {listed}");
    }
    Ok(paths)
}

/// How often to print progress to stderr (every N items).
pub(crate) const EML_PROGRESS_EVERY: u64 = 5000;

/// Print progress to stderr every [`EML_PROGRESS_EVERY`] items (and at the end).
pub(crate) fn report_progress(verbose: bool, label: &str, processed: u64, total: u64) {
    if !verbose || total == 0 {
        return;
    }
    let every = EML_PROGRESS_EVERY;
    if processed == total || (every > 0 && processed.is_multiple_of(every)) {
        eprintln!("{label}: {processed} / {total}");
    }
}

/// Convert SMS Backup+ EML tree(s) into SMS NDJSON (`message_json::sms`).
///
/// `inputs`: one or more `.eml` files or directories to scan (merged, then deduped
/// by path).
/// `contacts_path`: optional CSV for name→phone reverse lookup (`None` = no book).
/// `name_mapping_path`: optional CSV mapping incorrect EML names → correct contact names.
pub fn convert_export<P: AsRef<Path>>(
    inputs: &[P],
    output_dir: &Path,
    owner_phones: &[String],
    owner_emails: &[String],
    contacts_path: Option<&Path>,
    name_mapping_path: Option<&Path>,
    verbose: bool,
) -> Result<ExportReport> {
    if owner_phones.is_empty() {
        bail!("owner.phones must not be empty");
    }
    let owner = owner_digits(&owner_phones[0]);
    let owner_e164 = to_e164(&owner);
    let (contacts, _) = ContactsBook::load_optional(contacts_path)?;
    let (name_mapping, _) = NameMapping::load_optional(name_mapping_path)?;
    let mut report = ExportReport::default();
    let mut conversations: BTreeMap<String, PendingConversation> = BTreeMap::new();

    fs::create_dir_all(output_dir)?;
    for entry in fs::read_dir(output_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let _ = fs::remove_file(&path);
        }
    }
    let attachments_dir = output_dir.join("attachments");
    fs::create_dir_all(&attachments_dir)?;

    let eml_paths = collect_eml_paths(inputs)?;
    let total = eml_paths.len() as u64;
    if verbose {
        eprintln!("scanning {total} .eml files");
    }
    for (idx, eml_path) in eml_paths.into_iter().enumerate() {
        report_progress(verbose, "scanned", (idx + 1) as u64, total);
        let bytes = match std::fs::read(&eml_path) {
            Ok(b) => b,
            Err(err) => {
                report.errors.push(format!("{}: {err}", eml_path.display()));
                continue;
            }
        };
        let mail = match mailparse::parse_mail(&bytes) {
            Ok(m) => m,
            Err(err) => {
                report.skipped_not_sms_backup_plus += 1;
                report
                    .errors
                    .push(format!("{}: parse EML: {err}", eml_path.display()));
                continue;
            }
        };

        if is_archive_eml(&mail) {
            match parse_archive_eml_mail(&eml_path, &mail, &owner) {
                Ok((mut msgs, skipped_dates)) => {
                    report.archive_eml += 1;
                    report.skipped_invalid_date += skipped_dates;
                    for msg in &mut msgs {
                        let _ = apply_name_mapping(msg, &name_mapping);
                        let _ = fill_unknown_phone(msg, &contacts);
                    }
                    for msg in msgs {
                        if msg.chat_key == "Unknown" {
                            report.skipped_not_sms_backup_plus += 1;
                            continue;
                        }
                        match write_attachments(&msg.attachments, &attachments_dir, &mut report) {
                            Ok(atts) => add_message(&mut conversations, msg, atts, &mut report),
                            Err(err) => report
                                .errors
                                .push(format!("{}: {err:#}", eml_path.display())),
                        }
                    }
                }
                Err(err) => {
                    report.skipped_not_sms_backup_plus += 1;
                    report
                        .errors
                        .push(format!("{}: {err:#}", eml_path.display()));
                }
            }
            continue;
        }

        // Flat single-message SMS Backup+ EML
        if is_flat_sms_eml(&mail) {
            match parse_flat_eml_mail(&eml_path, &mail, &owner, owner_emails) {
                Ok(Some(mut msg)) => {
                    let _ = apply_name_mapping(&mut msg, &name_mapping);
                    let _ = fill_unknown_phone(&mut msg, &contacts);
                    if msg.chat_key == "Unknown" {
                        report.skipped_not_sms_backup_plus += 1;
                    } else {
                        report.flat_eml += 1;
                        match write_attachments(&msg.attachments, &attachments_dir, &mut report) {
                            Ok(atts) => add_message(&mut conversations, msg, atts, &mut report),
                            Err(err) => report
                                .errors
                                .push(format!("{}: {err:#}", eml_path.display())),
                        }
                    }
                }
                Ok(None) => report.skipped_not_sms_backup_plus += 1,
                Err(err) => {
                    report.skipped_not_sms_backup_plus += 1;
                    report
                        .errors
                        .push(format!("{}: {err:#}", eml_path.display()));
                }
            }
        } else {
            report.skipped_not_sms_backup_plus += 1;
        }
    }

    let exported_at = Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    for (chat_id, mut convo) in conversations {
        write_conversation(output_dir, &chat_id, &mut convo, &exported_at, &owner_e164)?;
        report.conversations += 1;
    }

    Ok(report)
}
