//! Convert SMS Backup+ `.eml` trees into per-conversation CSV.

use crate::archive::parse_archive_eml_mail;
use crate::contacts::{ContactsBook, NameMapping, apply_name_mapping, fill_unknown_phone};
use crate::flat_eml::{is_archive_eml, is_flat_sms_eml, parse_flat_eml_mail};
use crate::identity::{
    chat_id_for, cover_identity, local_datetime_from_secs, safe_stem, timestamp_ms,
};
use crate::phone::{owner_digits, to_e164};
use crate::types::{AttachmentBlob, ParsedMessage};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

/// Columns this exporter actually fills. Shared names match imessage-csv where
/// the concept exists; unused iMessage-only columns are omitted (not a universal schema).
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
    // Backup+-only
    "export_source",
    "source_kind",
    "smssync_id",
    "date_ms",
    "contact_name",
    "android_type",
    "eml_path",
];

const EXPORT_SOURCE: &str = "sms-backup-plus";

#[derive(Debug, Default)]
pub struct ExportReport {
    pub conversations: u64,
    pub flat_eml: u64,
    pub archive_eml: u64,
    pub messages: u64,
    pub messages_before_dedupe: u64,
    pub duplicates_dropped: u64,
    pub attachments_saved: u64,
    pub sent: u64,
    pub received: u64,
    /// `.eml` files that are not SMS Backup+ shaped (or unparseable as SMS).
    pub skipped_not_sms_backup_plus: u64,
    /// Messages kept under the `unknown` chat stem (no usable peer phone).
    pub unknown_chat_messages: u64,
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
    sender_display_name: Option<String>,
    text: String,
    attachments: Vec<PendingAttachment>,
    source_kind: String,
    smssync_id: String,
    date_ms: String,
    contact_name: String,
    android_type: String,
    eml_path: String,
}

#[derive(Debug, Default)]
struct PendingConversation {
    conversation_type: String,
    group_title: Option<String>,
    messages: Vec<PendingMessage>,
    /// Fingerprint → index in `messages` (online dedupe; keep earliest `sort_key`).
    by_identity: HashMap<String, usize>,
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

fn format_local_ts(secs: i64) -> (String, String, String) {
    let local = local_datetime_from_secs(secs);
    let utc = local.with_timezone(&Utc);
    let display = local.format("%b %e, %Y %I:%M:%S %p").to_string();
    (
        local.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        display,
    )
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

fn safe_filename(chat_id: &str) -> String {
    format!("{}.csv", safe_stem(chat_id))
}

fn relative_eml_path(eml_path: &Path, inputs: &[impl AsRef<Path>]) -> String {
    for root in inputs {
        let root = root.as_ref();
        if let Ok(rel) = eml_path.strip_prefix(root) {
            return rel.display().to_string();
        }
        if root.is_file() && eml_path == root {
            return root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_else(|| eml_path.to_str().unwrap_or(""))
                .to_string();
        }
    }
    eml_path.display().to_string()
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
    map: &'a mut HashMap<String, PendingConversation>,
    chat_id: &str,
    conversation_type: &str,
    group_title: Option<String>,
) -> &'a mut PendingConversation {
    // Avoid allocating a new String on every message for an existing chat.
    if !map.contains_key(chat_id) {
        map.insert(
            chat_id.to_string(),
            PendingConversation {
                conversation_type: conversation_type.to_string(),
                group_title,
                messages: Vec::new(),
                by_identity: HashMap::new(),
            },
        );
    }
    map.get_mut(chat_id).expect("just inserted or already present")
}

/// Prefer flat over archive (richer metadata); otherwise keep the earlier timestamp.
fn should_replace_kept(existing: &PendingMessage, incoming: &ParsedMessage) -> bool {
    let existing_flat = existing.source_kind == "flat";
    let incoming_flat = incoming.source_kind == "flat";
    if incoming_flat && !existing_flat {
        return true;
    }
    if !incoming_flat && existing_flat {
        return false;
    }
    if incoming_flat
        && existing_flat
        && incoming.smssync_id.as_ref().is_some_and(|s| !s.trim().is_empty())
        && existing.smssync_id.trim().is_empty()
    {
        return true;
    }
    incoming.timestamp_secs < existing.sort_key
}

/// Union attachment lists by content digest so flat↔archive dedupe does not drop media.
fn merge_attachments(into: &mut Vec<PendingAttachment>, from: Vec<PendingAttachment>) {
    let mut seen: HashSet<String> = into.iter().map(|a| a.digest_hex.clone()).collect();
    for att in from {
        if seen.insert(att.digest_hex.clone()) {
            into.push(att);
        }
    }
}

fn pending_from_parsed(msg: ParsedMessage, pending_atts: Vec<PendingAttachment>) -> PendingMessage {
    let date_ms = timestamp_ms(msg.timestamp_secs).to_string();
    let name = msg.name_hint.clone().unwrap_or_default();
    PendingMessage {
        sort_key: msg.timestamp_secs,
        is_from_me: msg.is_from_me,
        sender_digits: msg.sender_digits,
        sender_display_name: msg.name_hint,
        text: msg.text,
        attachments: pending_atts,
        source_kind: msg.source_kind,
        smssync_id: msg.smssync_id.unwrap_or_default(),
        date_ms,
        contact_name: name,
        android_type: msg.android_type,
        eml_path: msg.eml_path,
    }
}

fn add_message(
    conversations: &mut HashMap<String, PendingConversation>,
    msg: ParsedMessage,
    pending_atts: Vec<PendingAttachment>,
    report: &mut ExportReport,
) {
    let chat_id = chat_id_for(&msg);
    let dedupe_key = cover_identity(&msg);

    let convo = ensure_convo(
        conversations,
        &chat_id,
        &msg.conversation_type,
        msg.group_title.clone(),
    );

    report.messages_before_dedupe += 1;

    if let Some(&idx) = convo.by_identity.get(&dedupe_key) {
        report.duplicates_dropped += 1;
        if should_replace_kept(&convo.messages[idx], &msg) {
            let kept_atts = std::mem::take(&mut convo.messages[idx].attachments);
            let mut pending = pending_from_parsed(msg, pending_atts);
            merge_attachments(&mut pending.attachments, kept_atts);
            convo.messages[idx] = pending;
        } else {
            merge_attachments(&mut convo.messages[idx].attachments, pending_atts);
        }
        return;
    }

    if msg.is_from_me {
        report.sent += 1;
    } else {
        report.received += 1;
    }

    let idx = convo.messages.len();
    convo.by_identity.insert(dedupe_key, idx);
    convo.messages.push(pending_from_parsed(msg, pending_atts));
}

fn write_conversation(
    output_dir: &Path,
    chat_id: &str,
    convo: &mut PendingConversation,
    report: &mut ExportReport,
) -> Result<()> {
    if convo.messages.is_empty() {
        return Ok(());
    }
    convo.messages.sort_by(|a, b| {
        a.sort_key
            .partial_cmp(&b.sort_key)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

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
            msg.source_kind.as_str(),
            msg.smssync_id.as_str(),
            msg.date_ms.as_str(),
            msg.contact_name.as_str(),
            msg.android_type.as_str(),
            msg.eml_path.as_str(),
        ])
        .with_context(|| format!("write row {}", path.display()))?;
        report.messages += 1;
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

fn collect_eml_paths<P: AsRef<Path>>(inputs: &[P]) -> Result<Vec<PathBuf>> {
    if inputs.is_empty() {
        bail!("at least one --input path is required");
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
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

const EML_PROGRESS_EVERY: u64 = 5000;

fn vlog(verbose: bool, msg: impl AsRef<str>) {
    if verbose {
        eprintln!("{}", msg.as_ref());
    }
}

fn report_progress(verbose: bool, label: &str, processed: u64, total: u64) {
    if !verbose || total == 0 {
        return;
    }
    let every = EML_PROGRESS_EVERY;
    if processed == total || (every > 0 && processed.is_multiple_of(every)) {
        eprintln!("{label}: {processed} / {total}");
    }
}

/// Convert SMS Backup+ EML tree(s) into per-conversation CSV.
///
/// Deduplication runs while scanning, using [`cover_identity`] (second-floored
/// chat + direction + text) so archive and flat copies of the same SMS collapse.
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
    let mut owner_set = HashSet::new();
    for phone in owner_phones {
        let d = owner_digits(phone);
        if d != "Unknown" {
            owner_set.insert(d);
        }
    }
    if owner_set.is_empty() {
        bail!("owner.phones have no usable digits");
    }
    let (contacts, contacts_loaded) = ContactsBook::load_optional(contacts_path)?;
    let (name_mapping, mapping_loaded) = NameMapping::load_optional(name_mapping_path)?;
    let mut report = ExportReport::default();
    let mut conversations: HashMap<String, PendingConversation> = HashMap::new();

    vlog(verbose, format!("owner phones: {}", owner_set.len()));
    vlog(verbose, format!("owner emails: {}", owner_emails.len()));
    match &contacts_loaded {
        Some(p) => vlog(verbose, format!("contacts: {}", p.display())),
        None => vlog(verbose, "contacts: (none)"),
    }
    match &mapping_loaded {
        Some(p) => vlog(verbose, format!("name-mapping: {}", p.display())),
        None => vlog(verbose, "name-mapping: (none)"),
    }
    vlog(verbose, format!("output: {}", output_dir.display()));

    fs::create_dir_all(output_dir)?;
    clean_previous_csv(output_dir)?;
    let attachments_dir = output_dir.join("attachments");
    fs::create_dir_all(&attachments_dir)?;

    let eml_paths = collect_eml_paths(inputs)?;
    let total = eml_paths.len() as u64;
    vlog(verbose, format!("scanning {total} .eml files"));
    // Pre-size for typical 1:1 chat counts; grows as needed.
    conversations.reserve((total / 4).min(50_000) as usize);
    for (idx, eml_path) in eml_paths.into_iter().enumerate() {
        report_progress(verbose, "scanned", (idx + 1) as u64, total);
        let rel_path = relative_eml_path(&eml_path, inputs);
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
            match parse_archive_eml_mail(&eml_path, &mail) {
                Ok((mut msgs, skipped_dates)) => {
                    report.archive_eml += 1;
                    report.skipped_invalid_date += skipped_dates;
                    for msg in &mut msgs {
                        msg.eml_path = rel_path.clone();
                        let _ = apply_name_mapping(msg, &name_mapping);
                        let _ = fill_unknown_phone(msg, &contacts);
                    }
                    for msg in msgs {
                        if msg.chat_key == "Unknown" {
                            report.unknown_chat_messages += 1;
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

        if is_flat_sms_eml(&mail) {
            match parse_flat_eml_mail(&eml_path, &mail, &owner_set, owner_emails) {
                Ok(Some(mut msg)) => {
                    msg.eml_path = rel_path;
                    let _ = apply_name_mapping(&mut msg, &name_mapping);
                    let _ = fill_unknown_phone(&mut msg, &contacts);
                    report.flat_eml += 1;
                    if msg.chat_key == "Unknown" {
                        report.unknown_chat_messages += 1;
                    }
                    match write_attachments(&msg.attachments, &attachments_dir, &mut report) {
                        Ok(atts) => add_message(&mut conversations, msg, atts, &mut report),
                        Err(err) => report
                            .errors
                            .push(format!("{}: {err:#}", eml_path.display())),
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

    vlog(
        verbose,
        format!(
            "parsed: flat_eml={} archive_eml={} messages={} unknown_chat={} skipped_not_sms_backup_plus={} skipped_bad_date={}",
            report.flat_eml,
            report.archive_eml,
            report.messages_before_dedupe,
            report.unknown_chat_messages,
            report.skipped_not_sms_backup_plus,
            report.skipped_invalid_date
        ),
    );

    let convo_total = conversations.len() as u64;
    vlog(
        verbose,
        format!(
            "writing {convo_total} conversation CSV files (duplicates dropped so far: {})",
            report.duplicates_dropped
        ),
    );
    let mut written = 0u64;
    for (chat_id, mut convo) in conversations {
        write_conversation(output_dir, &chat_id, &mut convo, &mut report)?;
        if !convo.messages.is_empty() {
            report.conversations += 1;
        }
        written += 1;
        report_progress(verbose, "wrote", written, convo_total);
    }

    vlog(
        verbose,
        format!(
            "done: conversations={} messages={} duplicates_dropped={} attachments={}",
            report.conversations,
            report.messages,
            report.duplicates_dropped,
            report.attachments_saved
        ),
    );
    if verbose && !report.errors.is_empty() {
        eprintln!("errors: {}", report.errors.len());
        for err in report.errors.iter().take(20) {
            eprintln!("  {err}");
        }
        if report.errors.len() > 20 {
            eprintln!("  … and {} more", report.errors.len() - 20);
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_attachments_unions_by_digest() {
        let mut into = vec![PendingAttachment {
            rel_path: "attachments/a.jpg".into(),
            original_name: Some("a.jpg".into()),
            mime_type: Some("image/jpeg".into()),
            digest_hex: "aaa".into(),
        }];
        let from = vec![
            PendingAttachment {
                rel_path: "attachments/a.jpg".into(),
                original_name: Some("a.jpg".into()),
                mime_type: Some("image/jpeg".into()),
                digest_hex: "aaa".into(),
            },
            PendingAttachment {
                rel_path: "attachments/b.jpg".into(),
                original_name: Some("b.jpg".into()),
                mime_type: Some("image/jpeg".into()),
                digest_hex: "bbb".into(),
            },
        ];
        merge_attachments(&mut into, from);
        assert_eq!(into.len(), 2);
        assert_eq!(into[1].digest_hex, "bbb");
    }
}
