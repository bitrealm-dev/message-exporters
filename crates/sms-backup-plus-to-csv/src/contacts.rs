//! Contact name mapping and phone reverse-lookup from CSV files.

use crate::types::ParsedMessage;
use anyhow::{Context, Result};
use message_phone::sanitize_number;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Incorrect EML export name → correct contact display name.
#[derive(Debug, Default, Clone)]
pub(crate) struct NameMapping {
    /// Normalized incorrect name → correct display name (as written in CSV).
    incorrect_to_correct: HashMap<String, String>,
}

impl NameMapping {
    pub(crate) fn empty() -> Self {
        Self {
            incorrect_to_correct: HashMap::new(),
        }
    }

    /// Load `correct_name,incorrect_name` CSV.
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let file =
            File::open(path).with_context(|| format!("open name mapping {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let header = lines.next().transpose()?.unwrap_or_default();
        let header_l = header.to_ascii_lowercase();
        if !header_l.contains("correct_name") || !header_l.contains("incorrect_name") {
            anyhow::bail!(
                "name mapping CSV {} missing expected header correct_name,incorrect_name",
                path.display()
            );
        }

        let mut mapping = Self::empty();
        for (idx, line) in lines.enumerate() {
            let line = line.with_context(|| format!("read name mapping line {}", idx + 2))?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts = split_csv_line(line);
            if parts.len() < 2 {
                continue;
            }
            let correct = collapse_inner_whitespace(parts[0].trim());
            let incorrect = collapse_inner_whitespace(parts[1].trim());
            // Empty correct_name means “no usable contact” — do not map.
            if correct.is_empty() || incorrect.is_empty() {
                continue;
            }
            let key = normalize_name_key(&incorrect);
            if key.is_empty() {
                continue;
            }
            mapping.incorrect_to_correct.entry(key).or_insert(correct);
        }
        Ok(mapping)
    }

    pub(crate) fn load_optional(path: Option<&Path>) -> Result<(Self, Option<std::path::PathBuf>)> {
        match path {
            Some(path) => Ok((Self::load(path)?, Some(path.to_path_buf()))),
            None => Ok((Self::empty(), None)),
        }
    }

    /// If `eml_name` is an incorrect export name, return the correct display name.
    pub(crate) fn correct_name(&self, eml_name: &str) -> Option<&str> {
        let key = normalize_name_key(eml_name);
        if key.is_empty() {
            return None;
        }
        self.incorrect_to_correct.get(&key).map(String::as_str)
    }
}

/// Name → phone digits loaded from `phones,first_name,last_name` CSV.
#[derive(Debug, Default, Clone)]
pub(crate) struct ContactsBook {
    /// Normalized lookup key → phone digits (non-empty sanitized).
    by_name: HashMap<String, String>,
}

impl ContactsBook {
    pub(crate) fn empty() -> Self {
        Self {
            by_name: HashMap::new(),
        }
    }

    /// Load contacts from CSV. Errors if the file cannot be read or parsed.
    ///
    /// Accepts vault-shaped headers (`phones,first_name,last_name,exclude,…`).
    /// When an `exclude` column is present, rows with true/1/yes are skipped.
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("open contacts {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let header = lines.next().transpose()?.unwrap_or_default();
        let header_cols: Vec<String> = split_csv_line(&header)
            .into_iter()
            .map(|c| c.trim().to_ascii_lowercase())
            .collect();
        let phones_i = header_cols.iter().position(|c| c == "phones");
        let first_i = header_cols.iter().position(|c| c == "first_name");
        let last_i = header_cols.iter().position(|c| c == "last_name");
        let exclude_i = header_cols.iter().position(|c| c == "exclude");
        if phones_i.is_none() || first_i.is_none() {
            anyhow::bail!(
                "contacts CSV {} missing expected header phones,first_name,last_name",
                path.display()
            );
        }
        let phones_i = phones_i.unwrap();
        let first_i = first_i.unwrap();
        let last_i = last_i.unwrap_or(usize::MAX);

        let mut book = Self::empty();
        for (idx, line) in lines.enumerate() {
            let line = line.with_context(|| format!("read contacts line {}", idx + 2))?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts = split_csv_line(line);
            if let Some(ei) = exclude_i {
                if parse_exclude(parts.get(ei).map(String::as_str).unwrap_or("")) {
                    continue;
                }
            }
            let phones_raw = parts.get(phones_i).map(String::as_str).unwrap_or("");
            let first = parts.get(first_i).map(String::as_str).unwrap_or("");
            let last = if last_i == usize::MAX {
                ""
            } else {
                parts.get(last_i).map(String::as_str).unwrap_or("")
            };
            // Prefer clean rows: skip indexing dirty last_name markers.
            if last.contains("__") {
                continue;
            }
            let Some(phone) = first_valid_phone(phones_raw) else {
                continue;
            };
            let first = collapse_inner_whitespace(first.trim());
            let last = collapse_inner_whitespace(last.trim());
            if first.is_empty() && last.is_empty() {
                continue;
            }
            if last.is_empty() {
                book.insert_first(normalize_name_key(&first), phone);
            } else {
                let full = format!("{first} {last}");
                book.insert_first(normalize_name_key(&full), phone);
            }
        }
        Ok(book)
    }

    /// Load from an explicit path, or return an empty book when `None`.
    pub(crate) fn load_optional(path: Option<&Path>) -> Result<(Self, Option<std::path::PathBuf>)> {
        match path {
            Some(path) => {
                let book = Self::load(path)?;
                Ok((book, Some(path.to_path_buf())))
            }
            None => Ok((Self::empty(), None)),
        }
    }

    fn insert_first(&mut self, key: String, phone: String) {
        if key.is_empty() {
            return;
        }
        self.by_name.entry(key).or_insert(phone);
    }

    /// Look up digits for a display / export name. None if no match.
    pub(crate) fn lookup_phone(&self, name: &str) -> Option<String> {
        let key = normalize_name_key(name);
        if key.is_empty() {
            return None;
        }
        self.by_name.get(&key).cloned()
    }
}

/// Fill empty peer phone from the contacts book using current `name_hint`.
///
/// Returns `Some((display_name, phone))` when a phone fill happened.
/// Call [`apply_name_mapping`] first so aliases resolve to the contacts CSV name.
pub(crate) fn fill_unknown_phone(
    msg: &mut ParsedMessage,
    book: &ContactsBook,
) -> Option<(String, String)> {
    if !msg.chat_key.is_empty() {
        return None;
    }
    let display = msg
        .name_hint
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let phone = book.lookup_phone(&display)?;
    msg.chat_key = phone.clone();
    if !msg.is_from_me {
        msg.sender_digits = Some(phone.clone());
    }
    msg.participant_digits = vec![(phone.clone(), Some(display.clone()))];
    Some((display, phone))
}

/// Rewrite `name_hint` when the EML name appears as `incorrect_name` in the mapping.
///
/// Returns `Some((from, to))` when the hint changed.
pub(crate) fn apply_name_mapping(
    msg: &mut ParsedMessage,
    mapping: &NameMapping,
) -> Option<(String, String)> {
    let raw = msg
        .name_hint
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?
        .to_string();
    let correct = mapping.correct_name(&raw)?.to_string();
    if normalize_name_key(&raw) == normalize_name_key(&correct) {
        return None;
    }
    msg.name_hint = Some(correct.clone());
    for (_digits, name) in &mut msg.participant_digits {
        if name
            .as_deref()
            .is_some_and(|n| normalize_name_key(n) == normalize_name_key(&raw))
        {
            *name = Some(correct.clone());
        }
    }
    Some((raw, correct))
}

/// Normalize a contact / export name for map lookup.
pub(crate) fn normalize_name_key(name: &str) -> String {
    let mut s = name.trim().to_string();
    // Strip trailing __SUFFIX markers (e.g. Jordan_Alias__SKIP).
    if let Some(idx) = s.find("__") {
        s.truncate(idx);
    }
    s = s.replace('_', " ");
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn collapse_inner_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn first_valid_phone(phones_raw: &str) -> Option<String> {
    for part in phones_raw.split(';') {
        if let Some(digits) = sanitize_number(part.trim()) {
            return Some(digits);
        }
    }
    None
}

fn parse_exclude(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y"
    )
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if in_quotes && chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
            }
            ',' if !in_quotes => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_csv(dir: &tempfile::TempDir, name: &str, body: &str) -> std::path::PathBuf {
        let path = dir.path().join(name);
        let mut f = File::create(&path).unwrap();
        write!(f, "{body}").unwrap();
        path
    }

    #[test]
    fn loads_and_looks_up_full_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_csv(
            &dir,
            "contacts.csv",
            "phones,first_name,last_name\n\
15555550122,Sam,Example\n\
+15555550133;+15555550144,Pat,Contact\n",
        );
        let book = ContactsBook::load(&path).unwrap();
        assert_eq!(
            book.lookup_phone("Sam Example").as_deref(),
            Some("5555550122")
        );
        assert_eq!(
            book.lookup_phone("sam  example").as_deref(),
            Some("5555550122")
        );
        assert_eq!(
            book.lookup_phone("Pat Contact").as_deref(),
            Some("5555550133")
        );
    }

    #[test]
    fn skips_excluded_rows_in_vault_shaped_csv() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_csv(
            &dir,
            "contacts.csv",
            "phones,first_name,last_name,exclude,group_1\n\
+15555550100,Ada,Lovelace,false,Example\n\
+15555550999,Skip,Me,true,Example\n\
+15555550101,Grace,Hopper,1,Work\n",
        );
        let book = ContactsBook::load(&path).unwrap();
        assert_eq!(
            book.lookup_phone("Ada Lovelace").as_deref(),
            Some("5555550100")
        );
        assert!(book.lookup_phone("Skip Me").is_none());
        assert!(book.lookup_phone("Grace Hopper").is_none());
    }

    #[test]
    fn strips_underscore_suffix_on_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_csv(
            &dir,
            "contacts.csv",
            "phones,first_name,last_name\n\
15555550144,Jordan,Alias\n\
15555550144,Jordan,Alias__SKIP__\n",
        );
        let book = ContactsBook::load(&path).unwrap();
        assert_eq!(
            book.lookup_phone("Jordan_Alias__SKIP").as_deref(),
            Some("5555550144")
        );
    }

    #[test]
    fn empty_last_name_indexes_first_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_csv(
            &dir,
            "contacts.csv",
            "phones,first_name,last_name\n\
+15555550155;+15555550166,Solo Biz,\n",
        );
        let book = ContactsBook::load(&path).unwrap();
        assert_eq!(
            book.lookup_phone("Solo Biz").as_deref(),
            Some("5555550155")
        );
    }

    #[test]
    fn name_mapping_translates_then_looks_up_phone() {
        let dir = tempfile::tempdir().unwrap();
        let contacts = write_csv(
            &dir,
            "contacts.csv",
            "phones,first_name,last_name\n\
15555550144,Jordan,Alias\n",
        );
        let mapping_path = write_csv(
            &dir,
            "mapping.csv",
            "correct_name,incorrect_name\n\
Jordan Alias,Jordan Alias (SKIP)\n\
,OrphanLabel\n",
        );
        let book = ContactsBook::load(&contacts).unwrap();
        let mapping = NameMapping::load(&mapping_path).unwrap();
        assert_eq!(
            mapping.correct_name("Jordan Alias (SKIP)"),
            Some("Jordan Alias")
        );
        assert!(mapping.correct_name("OrphanLabel").is_none());

        let mut msg = ParsedMessage {
            chat_key: String::new(),
            conversation_type: "individual".into(),
            group_title: None,
            participant_digits: vec![],
            timestamp_secs: 1.0,
            is_from_me: false,
            sender_digits: None,
            text: "hi".into(),
            attachments: vec![],
            name_hint: Some("Jordan Alias (SKIP)".into()),
            smssync_id: None,
            source_kind: "flat".into(),
            android_type: String::new(),
            eml_path: String::new(),
        };
        let mapped = apply_name_mapping(&mut msg, &mapping).unwrap();
        assert_eq!(mapped.0, "Jordan Alias (SKIP)");
        assert_eq!(mapped.1, "Jordan Alias");
        let hit = fill_unknown_phone(&mut msg, &book).unwrap();
        assert_eq!(hit.0, "Jordan Alias");
        assert_eq!(hit.1, "5555550144");
        assert_eq!(msg.name_hint.as_deref(), Some("Jordan Alias"));
        assert_eq!(msg.chat_key, "5555550144");
    }

    #[test]
    fn fill_unknown_phone_updates_message() {
        let mut book = ContactsBook::empty();
        book.insert_first(normalize_name_key("Sam Example"), "5555550122".into());
        let mut msg = ParsedMessage {
            chat_key: String::new(),
            conversation_type: "individual".into(),
            group_title: None,
            participant_digits: vec![],
            timestamp_secs: 1.0,
            is_from_me: false,
            sender_digits: None,
            text: "hi".into(),
            attachments: vec![],
            name_hint: Some("Sam Example".into()),
            smssync_id: None,
            source_kind: "flat".into(),
            android_type: String::new(),
            eml_path: String::new(),
        };
        let hit = fill_unknown_phone(&mut msg, &book).unwrap();
        assert_eq!(hit.1, "5555550122");
        assert_eq!(msg.chat_key, "5555550122");
        assert_eq!(msg.sender_digits.as_deref(), Some("5555550122"));
    }
}
