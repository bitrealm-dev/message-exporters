//! Walk a messy SMS Backup+ EML tree and write one clean copy of each text.
//!
//! # Two kinds of input file
//!
//! - **Flat** — one text per `.eml`, e.g. `2012-05-24_142031_SMS_-14075551234.eml`.
//! - **Archive** — many texts in one file (subject like `SMS archive with Alice`),
//!   e.g. `SMS with Alice (2011-2013).eml`.
//!
//! # What this does
//!
//! 1. Scan every `.eml` under the input folder(s).
//! 2. For duplicate flats (same fingerprint — see [`crate::identity`]), keep one
//!    winner (prefer attachments, then smssync headers, then larger file, then
//!    path order).
//! 3. Copy winners into `{output}/{YYYY}/` with clear names.
//! 4. For archive lines that have no matching flat, create a new flat file
//!    (`GENERATED` in the log). Those are not copies of a backup file.
//! 5. Flat or archive messages whose phone we cannot resolve go under
//!    `{output}/junk/{YYYY}/`, and their names are listed in
//!    `junk/unresolved_names.txt`.
//! 6. Unparseable SMS-shaped (or broken) `.eml` files are copied to
//!    `{output}/unparseable/` (next to `junk/`).
//!
//! Writes `dedupe.log` at the output root with keep/drop/copy/GENERATED lines.

use crate::archive::parse_archive_eml_mail;
use crate::contacts::{
    ContactsBook, NameMapping, apply_name_mapping, display_name_for_unresolved, fill_unknown_phone,
};
use crate::emit::{collect_eml_paths, report_progress};
use crate::flat_eml::{is_archive_eml, is_flat_sms_eml, parse_flat_eml_mail};
use crate::identity::{
    chat_id_for, content_identity, cover_identity, local_datetime_from_secs, message_identity,
    safe_stem, short_id,
};
use crate::phone::owner_digits;
use crate::types::ParsedMessage;
use crate::write_flat_eml::write_flat_eml_bytes;
use anyhow::{Context, Result};
use chrono::Local;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct DedupeReport {
    pub flat_seen: u64,
    pub archive_eml: u64,
    pub unique_flat: u64,
    pub copied: u64,
    pub duplicates_dropped: u64,
    pub archive_overlaps: u64,
    pub archive_only: u64,
    pub archive_generated: u64,
    pub archive_generated_junk: u64,
    pub contacts_resolved: u64,
    pub names_mapped: u64,
    pub unresolved_names: u64,
    /// Path to junk/unresolved_names.txt when written.
    pub unresolved_names_path: Option<PathBuf>,
    /// Flat SMS with unknown phone (no contact match), copied to junk/.
    pub flat_unknown_junk: u64,
    pub skipped_not_sms: u64,
    pub skipped_unparseable: u64,
    /// Path to the unparseable/ directory when created.
    pub unparseable_dir: Option<PathBuf>,
    pub errors: Vec<String>,
    /// Path to the processing log written under the output directory.
    pub log_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct FlatCandidate {
    path: PathBuf,
    identity: String,
    msg: ParsedMessage,
    attachment_count: usize,
    file_size: u64,
    has_smssync_id: bool,
    /// Other source paths that lost to this winner for the same identity.
    dropped_sources: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct ArchiveEntry {
    sources: Vec<PathBuf>,
    msg: ParsedMessage,
}

struct ProcessLog {
    lines: Vec<String>,
}

impl ProcessLog {
    fn new() -> Self {
        Self { lines: Vec::new() }
    }

    fn line(&mut self, s: impl Into<String>) {
        self.lines.push(s.into());
    }

    fn write_to(&self, path: &Path) -> Result<()> {
        let file = File::create(path).with_context(|| format!("create {}", path.display()))?;
        let mut file = BufWriter::with_capacity(1024 * 1024, file);
        for line in &self.lines {
            writeln!(file, "{line}")?;
        }
        file.flush()?;
        Ok(())
    }
}

fn prefer_reason(a: &FlatCandidate, b: &FlatCandidate) -> Option<&'static str> {
    // Some(reason) if `b` should replace `a`
    if b.attachment_count != a.attachment_count {
        return if b.attachment_count > a.attachment_count {
            Some("more_attachments")
        } else {
            None
        };
    }
    if b.has_smssync_id != a.has_smssync_id {
        return if b.has_smssync_id {
            Some("has_smssync_id")
        } else {
            None
        };
    }
    if b.file_size != a.file_size {
        return if b.file_size > a.file_size {
            Some("larger_file")
        } else {
            None
        };
    }
    if b.path < a.path {
        Some("lexicographically_smaller_path")
    } else {
        None
    }
}

fn format_ts_stem(secs: i64) -> String {
    local_datetime_from_secs(secs)
        .format("%Y%m%d_%H%M%S")
        .to_string()
}

fn message_year(msg: &ParsedMessage) -> String {
    local_datetime_from_secs(msg.timestamp_secs as i64)
        .format("%Y")
        .to_string()
}

fn is_year_dirname(name: &str) -> bool {
    name.len() == 4 && name.chars().all(|c| c.is_ascii_digit())
}

fn output_filename(msg: &ParsedMessage, identity: &str) -> String {
    let ts = format_ts_stem(msg.timestamp_secs as i64);
    let dir = if msg.is_from_me { "sent" } else { "recv" };
    let chat = if msg.chat_key == "Unknown" {
        let hint = msg
            .name_hint
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("unknown");
        safe_stem(hint)
    } else {
        safe_stem(&chat_id_for(msg))
    };
    let sid = short_id(identity);
    format!("{ts}_{dir}_{chat}_{sid}.eml")
}

/// Write under `dir` with a unique filename. `scope` (e.g. year) namespaces
/// collision tracking so the same base name can exist in different years.
fn unique_output_path(
    dir: &Path,
    scope: &str,
    base_name: &str,
    used: &mut HashSet<String>,
) -> Result<PathBuf> {
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let stem = base_name.trim_end_matches(".eml");
    let mut name = base_name.to_string();
    let mut n = 2u32;
    loop {
        let key = format!("{scope}/{name}");
        if used.insert(key) {
            return Ok(dir.join(name));
        }
        name = format!("{stem}_{n}.eml");
        n += 1;
    }
}

fn short_identity(identity: &str) -> String {
    let compact = identity.replace(['\r', '\n'], "\\n");
    let sid = short_id(identity);
    if compact.len() <= 80 {
        return compact;
    }
    let mut end = 77.min(compact.len());
    while end > 0 && !compact.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… (short={sid})", &compact[..end])
}

fn file_size_of(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn clear_eml_tree(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "junk" || name == "unparseable" {
                clear_eml_tree(&path)?;
            } else if is_year_dirname(name) {
                let _ = fs::remove_dir_all(&path);
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("eml"))
        {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
}

fn stash_unparseable(
    src: &Path,
    unparseable_dir: &Path,
    used: &mut HashSet<String>,
    reason: &str,
    log: &mut ProcessLog,
    report: &mut DedupeReport,
) -> Result<()> {
    report.skipped_unparseable += 1;
    fs::create_dir_all(unparseable_dir)?;
    let base = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("message.eml");
    let base = if base.to_ascii_lowercase().ends_with(".eml") {
        base.to_string()
    } else {
        format!("{base}.eml")
    };
    let dest = unique_output_path(unparseable_dir, "unparseable", &base, used)?;
    fs::copy(src, &dest)
        .with_context(|| format!("copy unparseable {} -> {}", src.display(), dest.display()))?;
    log.line(format!(
        "UNPARSEABLE {} -> {} ({reason})",
        src.display(),
        dest.display()
    ));
    Ok(())
}

/// Scan `inputs`, keep one flat per fingerprint, and write them under `output_dir`.
///
/// Steps match the module docs: scan → pick winners → copy flats under
/// `{YYYY}/` → generate archive-only flats ([`crate::write_flat_eml`]) → put
/// unknown phones in `junk/{YYYY}/`. Archive↔flat coverage uses
/// [`cover_identity`] (whole-second, no attachments).
///
/// Log keyword **`GENERATED`** means we created a new `.eml` here; it was not
/// copied from the backup tree.
///
/// `inputs`: one or more `.eml` files or directories to scan (merged, then
/// deduped by path).
/// `contacts_path`: optional CSV for name→phone reverse lookup (`None` = no book).
/// `name_mapping_path`: optional CSV mapping incorrect EML names → correct contact names.
pub fn dedupe_eml<P: AsRef<Path>>(
    inputs: &[P],
    output_dir: &Path,
    owner_phone: &str,
    owner_emails: &[String],
    contacts_path: Option<&Path>,
    name_mapping_path: Option<&Path>,
    verbose: bool,
) -> Result<DedupeReport> {
    let owner = owner_digits(owner_phone);
    let (contacts, contacts_loaded) = ContactsBook::load_optional(contacts_path)?;
    let (name_mapping, mapping_loaded) = NameMapping::load_optional(name_mapping_path)?;
    let mut report = DedupeReport::default();
    let mut winners: HashMap<String, FlatCandidate> = HashMap::new();
    let mut archive_entries: HashMap<String, ArchiveEntry> = HashMap::new();
    let mut unknown_flats: Vec<FlatCandidate> = Vec::new();
    let mut log = ProcessLog::new();
    let mut unresolved_names: BTreeSet<String> = BTreeSet::new();
    let mut mapped_unique: BTreeSet<(String, String)> = BTreeSet::new();
    let mut resolved_unique: BTreeSet<(String, String)> = BTreeSet::new();
    let mut used_unparseable_names = HashSet::new();

    let started = Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    log.line("# sms-backup-plus-exporter dedupe-eml log".to_string());
    log.line(format!("started: {started}"));
    for input in inputs {
        log.line(format!("input:   {}", input.as_ref().display()));
    }
    log.line(format!("output:  {}", output_dir.display()));
    log.line(format!("owner:   {owner_phone}"));
    match &contacts_loaded {
        Some(p) => log.line(format!(
            "contacts: {} ({} names)",
            p.display(),
            contacts.len()
        )),
        None => log.line("contacts: (none)"),
    }
    match &mapping_loaded {
        Some(p) => log.line(format!(
            "name_mapping: {} ({} aliases)",
            p.display(),
            name_mapping.len()
        )),
        None => log.line("name_mapping: (none)"),
    }
    log.line("");
    log.line("## scan");

    fs::create_dir_all(output_dir)?;
    clear_eml_tree(output_dir)?;
    let junk_dir = output_dir.join("junk");
    let unparseable_dir = output_dir.join("unparseable");
    fs::create_dir_all(&junk_dir)?;
    fs::create_dir_all(&unparseable_dir)?;
    report.unparseable_dir = Some(unparseable_dir.clone());

    let eml_paths = collect_eml_paths(inputs)?;
    let total = eml_paths.len() as u64;
    if verbose {
        eprintln!("scanning {total} .eml files");
    }
    log.line(format!("eml_files: {total}"));
    for (idx, eml_path) in eml_paths.into_iter().enumerate() {
        report_progress(verbose, "scanned", (idx + 1) as u64, total);
        let bytes = match fs::read(&eml_path) {
            Ok(b) => b,
            Err(err) => {
                let msg = format!("{}: {err}", eml_path.display());
                report.errors.push(msg.clone());
                log.line(format!("ERROR  {msg}"));
                continue;
            }
        };
        let mail = match mailparse::parse_mail(&bytes) {
            Ok(m) => m,
            Err(err) => {
                let msg = format!("{}: parse EML: {err}", eml_path.display());
                report.errors.push(msg);
                stash_unparseable(
                    &eml_path,
                    &unparseable_dir,
                    &mut used_unparseable_names,
                    "unparseable EML",
                    &mut log,
                    &mut report,
                )?;
                continue;
            }
        };

        if is_archive_eml(&mail) {
            match parse_archive_eml_mail(&eml_path, &mail, &owner) {
                Ok((mut msgs, _)) => {
                    report.archive_eml += 1;
                    for msg in &mut msgs {
                        if let Some((from, to)) = apply_name_mapping(msg, &name_mapping)
                            && mapped_unique.insert((from.clone(), to.clone()))
                        {
                            log.line(format!("MAP     {from} -> {to}"));
                        }
                        if let Some((name, phone)) = fill_unknown_phone(msg, &contacts)
                            && resolved_unique.insert((name.clone(), phone.clone()))
                        {
                            log.line(format!("CONTACT {name} -> {phone}"));
                        }
                    }
                    log.line(format!(
                        "ARCHIVE {} ({} messages)",
                        eml_path.display(),
                        msgs.len()
                    ));
                    for msg in msgs {
                        let identity = content_identity(&msg);
                        archive_entries
                            .entry(identity)
                            .and_modify(|e| e.sources.push(eml_path.clone()))
                            .or_insert_with(|| ArchiveEntry {
                                sources: vec![eml_path.clone()],
                                msg,
                            });
                    }
                }
                Err(err) => {
                    let msg = format!("{}: {err:#}", eml_path.display());
                    report.errors.push(msg);
                    stash_unparseable(
                        &eml_path,
                        &unparseable_dir,
                        &mut used_unparseable_names,
                        "archive parse failed",
                        &mut log,
                        &mut report,
                    )?;
                }
            }
            continue;
        }

        if !is_flat_sms_eml(&mail) {
            report.skipped_not_sms += 1;
            continue;
        }

        match parse_flat_eml_mail(&eml_path, &mail, &owner, owner_emails) {
            Ok(Some(mut msg)) => {
                if let Some((from, to)) = apply_name_mapping(&mut msg, &name_mapping)
                    && mapped_unique.insert((from.clone(), to.clone()))
                {
                    log.line(format!("MAP     {from} -> {to}"));
                }
                if let Some((name, phone)) = fill_unknown_phone(&mut msg, &contacts)
                    && resolved_unique.insert((name.clone(), phone.clone()))
                {
                    log.line(format!("CONTACT {name} -> {phone}"));
                }
                if msg.chat_key == "Unknown" {
                    unresolved_names.insert(display_name_for_unresolved(&msg));
                    report.flat_unknown_junk += 1;
                    let identity = message_identity(&msg);
                    log.line(format!(
                        "JUNK   {} (unknown phone, no contact match)",
                        eml_path.display()
                    ));
                    unknown_flats.push(FlatCandidate {
                        path: eml_path.clone(),
                        identity,
                        attachment_count: msg.attachments.len(),
                        file_size: file_size_of(&eml_path),
                        has_smssync_id: msg
                            .smssync_id
                            .as_ref()
                            .is_some_and(|s| !s.trim().is_empty()),
                        msg,
                        dropped_sources: Vec::new(),
                    });
                    continue;
                }
                report.flat_seen += 1;
                let identity = message_identity(&msg);
                let candidate = FlatCandidate {
                    path: eml_path.clone(),
                    identity: identity.clone(),
                    attachment_count: msg.attachments.len(),
                    file_size: file_size_of(&eml_path),
                    has_smssync_id: msg
                        .smssync_id
                        .as_ref()
                        .is_some_and(|s| !s.trim().is_empty()),
                    msg,
                    dropped_sources: Vec::new(),
                };
                match winners.get_mut(&identity) {
                    Some(existing) => {
                        report.duplicates_dropped += 1;
                        let id_disp = short_identity(&identity);
                        if let Some(reason) = prefer_reason(existing, &candidate) {
                            log.line(format!(
                                "REPLACE identity={id_disp} keep={} drop={} reason={reason}",
                                candidate.path.display(),
                                existing.path.display(),
                            ));
                            let mut dropped = std::mem::take(&mut existing.dropped_sources);
                            dropped.push(existing.path.clone());
                            let mut new_winner = candidate;
                            new_winner.dropped_sources = dropped;
                            winners.insert(identity, new_winner);
                        } else {
                            let reason =
                                prefer_reason(&candidate, existing).unwrap_or("incumbent_wins");
                            log.line(format!(
                                "DROP   identity={id_disp} keep={} drop={} reason={reason}",
                                existing.path.display(),
                                candidate.path.display(),
                            ));
                            existing.dropped_sources.push(candidate.path);
                        }
                    }
                    None => {
                        // Skip KEEP lines — COPY at the end records winners. Logging every
                        // first sighting made dedupe.log huge and slowed large scans.
                        winners.insert(identity, candidate);
                    }
                }
            }
            Ok(None) => {
                stash_unparseable(
                    &eml_path,
                    &unparseable_dir,
                    &mut used_unparseable_names,
                    "flat parse returned none",
                    &mut log,
                    &mut report,
                )?;
            }
            Err(err) => {
                let msg = format!("{}: {err:#}", eml_path.display());
                report.errors.push(msg);
                stash_unparseable(
                    &eml_path,
                    &unparseable_dir,
                    &mut used_unparseable_names,
                    "flat parse error",
                    &mut log,
                    &mut report,
                )?;
            }
        }
    }

    report.names_mapped = mapped_unique.len() as u64;
    report.contacts_resolved = resolved_unique.len() as u64;

    let flat_cover_keys: HashSet<String> =
        winners.values().map(|c| cover_identity(&c.msg)).collect();

    log.line("");
    log.line("## archive coverage");
    let mut archive_only_msgs: Vec<ParsedMessage> = Vec::new();
    for (id, entry) in &archive_entries {
        let cover = cover_identity(&entry.msg);
        if flat_cover_keys.contains(&cover) {
            report.archive_overlaps += 1;
            // Skip per-message OVERLAP lines — summary count is enough; logging
            // tens of thousands of them dominated runtime and log size.
        } else {
            report.archive_only += 1;
            let id_disp = short_identity(id);
            let src = entry
                .sources
                .first()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            log.line(format!(
                "ARCHIVE_ONLY identity={id_disp} archive=[{src}] (will generate flat)"
            ));
            archive_only_msgs.push(entry.msg.clone());
        }
    }
    log.line(format!(
        "archive_overlaps: {} (per-message OVERLAP lines omitted)",
        report.archive_overlaps
    ));
    if archive_entries.is_empty() {
        log.line("(no archive messages)");
    }

    report.unique_flat = winners.len() as u64;

    log.line("");
    log.line("## copy");
    let mut used_names = HashSet::new();
    let mut used_junk_names = HashSet::new();
    let mut ordered: Vec<_> = winners.into_values().collect();
    ordered.sort_by(|a, b| {
        a.msg
            .timestamp_secs
            .partial_cmp(&b.msg.timestamp_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    if verbose {
        eprintln!("copying {} unique flats", ordered.len());
    }
    for (idx, cand) in ordered.into_iter().enumerate() {
        let year = message_year(&cand.msg);
        let base = output_filename(&cand.msg, &cand.identity);
        let dest = unique_output_path(
            &output_dir.join(&year),
            &year,
            &base,
            &mut used_names,
        )?;
        fs::copy(&cand.path, &dest)
            .with_context(|| format!("copy {} -> {}", cand.path.display(), dest.display()))?;
        report.copied += 1;
        let src_name = cand
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        let dest_rel = dest
            .strip_prefix(output_dir)
            .unwrap_or(&dest)
            .display();
        log.line(format!("COPY   {src_name} -> {dest_rel}"));
        report_progress(verbose, "copied", (idx + 1) as u64, report.unique_flat.max(1));
    }

    unknown_flats.sort_by(|a, b| {
        a.msg
            .timestamp_secs
            .partial_cmp(&b.msg.timestamp_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });
    for cand in unknown_flats {
        let year = message_year(&cand.msg);
        let base = output_filename(&cand.msg, &cand.identity);
        let dest = unique_output_path(
            &junk_dir.join(&year),
            &year,
            &base,
            &mut used_junk_names,
        )?;
        fs::copy(&cand.path, &dest).with_context(|| {
            format!(
                "copy junk {} -> {}",
                cand.path.display(),
                dest.display()
            )
        })?;
        let id_disp = short_identity(&cand.identity);
        log.line(format!(
            "COPY   {} -> {} identity={id_disp} (junk/ unknown phone)",
            cand.path.display(),
            dest.display(),
        ));
    }

    log.line("");
    log.line("## generate archive-only");
    archive_only_msgs.sort_by(|a, b| {
        a.timestamp_secs
            .partial_cmp(&b.timestamp_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.chat_key.cmp(&b.chat_key))
            .then_with(|| a.text.cmp(&b.text))
    });

    let gen_total = archive_only_msgs.len() as u64;
    if verbose && gen_total > 0 {
        eprintln!("generating {gen_total} archive-only flats");
    }
    for (idx, msg) in archive_only_msgs.into_iter().enumerate() {
        let content_key = content_identity(&msg);
        let to_junk = msg.chat_key == "Unknown";
        if to_junk {
            unresolved_names.insert(display_name_for_unresolved(&msg));
        }
        let year = message_year(&msg);
        let base = output_filename(&msg, &content_key);
        let dest = if to_junk {
            unique_output_path(
                &junk_dir.join(&year),
                &year,
                &base,
                &mut used_junk_names,
            )?
        } else {
            unique_output_path(
                &output_dir.join(&year),
                &year,
                &base,
                &mut used_names,
            )?
        };
        let bytes = write_flat_eml_bytes(&msg, owner_phone);
        fs::write(&dest, &bytes).with_context(|| format!("write generated {}", dest.display()))?;
        report.archive_generated += 1;
        if to_junk {
            report.archive_generated_junk += 1;
        }
        let dest_rel = dest.strip_prefix(output_dir).unwrap_or(&dest).display();
        log.line(format!(
            "GENERATED {dest_rel}{}",
            if to_junk {
                " (junk/ unknown phone)"
            } else {
                ""
            }
        ));
        report_progress(verbose, "generated", (idx + 1) as u64, gen_total.max(1));
    }

    let unresolved_path = junk_dir.join("unresolved_names.txt");
    {
        let mut file = File::create(&unresolved_path)
            .with_context(|| format!("create {}", unresolved_path.display()))?;
        for name in &unresolved_names {
            writeln!(file, "{name}")?;
        }
    }
    report.unresolved_names = unresolved_names.len() as u64;
    report.unresolved_names_path = Some(unresolved_path.clone());
    log.line("");
    log.line(format!(
        "unresolved_names: {} -> {}",
        report.unresolved_names,
        unresolved_path.display()
    ));

    log.line("");
    log.line("## summary");
    log.line(format!("flat_seen:              {}", report.flat_seen));
    log.line(format!("unique_flat:            {}", report.unique_flat));
    log.line(format!("copied:                 {}", report.copied));
    log.line(format!(
        "duplicates_dropped:     {}",
        report.duplicates_dropped
    ));
    log.line(format!("archive_eml:            {}", report.archive_eml));
    log.line(format!(
        "archive_overlaps:       {}",
        report.archive_overlaps
    ));
    log.line(format!("archive_only:           {}", report.archive_only));
    log.line(format!(
        "archive_generated:      {}",
        report.archive_generated
    ));
    log.line(format!(
        "archive_generated_junk: {}",
        report.archive_generated_junk
    ));
    log.line(format!(
        "flat_unknown_junk:      {}",
        report.flat_unknown_junk
    ));
    log.line(format!(
        "contacts_resolved_unique: {}",
        report.contacts_resolved
    ));
    log.line(format!(
        "names_mapped_unique:    {}",
        report.names_mapped
    ));
    log.line(format!(
        "unresolved_names:       {}",
        report.unresolved_names
    ));
    log.line(format!(
        "skipped_not_sms:        {}",
        report.skipped_not_sms
    ));
    log.line(format!(
        "skipped_unparseable:    {} -> {}",
        report.skipped_unparseable,
        unparseable_dir.display()
    ));
    log.line(format!("errors:                 {}", report.errors.len()));
    for err in &report.errors {
        log.line(format!("  error: {err}"));
    }
    let finished = Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    log.line(format!("finished: {finished}"));

    let log_path = output_dir.join("dedupe.log");
    log.write_to(&log_path)?;
    report.log_path = Some(log_path);

    Ok(report)
}
