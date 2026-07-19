//! Bidirectional contacts index (name↔phone).

use crate::name::{collapse_inner_whitespace, is_blank_or_unknown_name, normalize_name_key};
use crate::vcf::{self, strip_tags};
use anyhow::{bail, Context, Result};
use message_phone::{sanitize_number, to_e164};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Name → phone digits and phone → display name.
#[derive(Debug, Default, Clone)]
pub struct ContactsBook {
    /// Normalized name key → sanitized phone digits.
    by_name: HashMap<String, String>,
    /// Sanitized phone digits → display name (`First Last` or first-only).
    by_phone: HashMap<String, String>,
}

impl ContactsBook {
    pub fn empty() -> Self {
        Self {
            by_name: HashMap::new(),
            by_phone: HashMap::new(),
        }
    }

    /// Load vault-shaped CSV: `phones,first_name,last_name[,exclude,…]`.
    pub fn load_csv(path: &Path) -> Result<Self> {
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
            bail!(
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
            if last.contains("__") {
                continue;
            }
            let phones = all_valid_phones(phones_raw);
            if phones.is_empty() {
                continue;
            }
            let first = collapse_inner_whitespace(first.trim());
            let last = collapse_inner_whitespace(last.trim());
            if first.is_empty() && last.is_empty() {
                continue;
            }
            let display = if last.is_empty() {
                first.clone()
            } else {
                format!("{first} {last}")
            };
            book.insert_entry(&display, &phones);
        }
        Ok(book)
    }

    /// Load contacts from a VCF file (FN/N + TEL).
    pub fn load_vcf(path: &Path) -> Result<Self> {
        let cards = vcf::parse_vcf(path)?;
        let mut book = Self::empty();
        for card in cards {
            let phones: Vec<String> = card
                .phones
                .iter()
                .filter_map(|p| sanitize_number(p))
                .collect();
            if phones.is_empty() {
                continue;
            }
            let first = strip_tags(&card.n_given);
            let last = strip_tags(&card.n_family);
            let fn_stripped = strip_tags(&card.fn_raw);
            let display = if !first.is_empty() || !last.is_empty() {
                if last.is_empty() {
                    first
                } else if first.is_empty() {
                    last
                } else {
                    format!("{first} {last}")
                }
            } else if !fn_stripped.is_empty() {
                fn_stripped
            } else {
                continue;
            };
            book.insert_entry(&display, &phones);
        }
        Ok(book)
    }

    /// Load an iMazing Contacts CSV export (wide address-book columns).
    ///
    /// Phones come from Mobile/Home/Work/Other (and fax) columns, plus `+E.164`
    /// tokens scraped from `Notes` (including `PROP-ID: +…`).
    pub fn load_imazing_contacts_csv(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
        let mut rdr = csv::ReaderBuilder::new()
            .flexible(true)
            .from_reader(file);
        let headers = rdr
            .headers()
            .with_context(|| format!("headers {}", path.display()))?
            .iter()
            .map(|h| h.trim().to_ascii_lowercase())
            .collect::<Vec<_>>();

        let first_i = headers.iter().position(|h| h == "first name");
        let middle_i = headers.iter().position(|h| h == "middle name");
        let last_i = headers.iter().position(|h| h == "last name");
        let notes_i = headers.iter().position(|h| h == "notes");
        let phone_cols: Vec<usize> = [
            "mobile phone",
            "home phone",
            "work phone",
            "other phone",
            "home fax",
            "work fax",
            "other fax",
        ]
        .iter()
        .filter_map(|name| headers.iter().position(|h| h == *name))
        .collect();

        if first_i.is_none() && phone_cols.is_empty() {
            bail!(
                "contacts CSV {} does not look like an iMazing Contacts export \
                 (expected First Name and/or phone columns)",
                path.display()
            );
        }

        let mut book = Self::empty();
        for (idx, rec) in rdr.records().enumerate() {
            let rec = rec.with_context(|| format!("row {} in {}", idx + 2, path.display()))?;
            let first = first_i
                .map(|i| rec.get(i).unwrap_or("").trim())
                .unwrap_or("");
            let middle = middle_i
                .map(|i| rec.get(i).unwrap_or("").trim())
                .unwrap_or("");
            let last = last_i
                .map(|i| rec.get(i).unwrap_or("").trim())
                .unwrap_or("");
            let mut name_parts = Vec::new();
            if !first.is_empty() {
                name_parts.push(first);
            }
            if !middle.is_empty() {
                name_parts.push(middle);
            }
            if !last.is_empty() {
                name_parts.push(last);
            }
            let display = collapse_inner_whitespace(&name_parts.join(" "));

            let mut phones = Vec::new();
            for &i in &phone_cols {
                push_phones_from_raw(rec.get(i).unwrap_or(""), &mut phones);
            }
            if let Some(ni) = notes_i {
                push_phones_from_raw(rec.get(ni).unwrap_or(""), &mut phones);
            }
            if phones.is_empty() {
                continue;
            }
            if display.is_empty() {
                continue;
            }
            book.insert_entry(&display, &phones);
        }
        Ok(book)
    }

    fn insert_entry(&mut self, display: &str, phones: &[String]) {
        let display = collapse_inner_whitespace(display);
        if display.is_empty() || phones.is_empty() {
            return;
        }
        let key = normalize_name_key(&display);
        if !key.is_empty() {
            self.by_name
                .entry(key)
                .or_insert_with(|| phones[0].clone());
        }
        for phone in phones {
            self.by_phone
                .entry(phone.clone())
                .or_insert_with(|| display.clone());
        }
    }

    /// Look up sanitized digits for a display / export name.
    pub fn lookup_phone_by_name(&self, name: &str) -> Option<String> {
        let key = normalize_name_key(name);
        if key.is_empty() {
            return None;
        }
        self.by_name.get(&key).cloned()
    }

    /// Look up display name for a phone (raw or sanitized).
    pub fn lookup_name_by_phone(&self, phone: &str) -> Option<&str> {
        let digits = sanitize_number(phone)?;
        self.by_phone.get(&digits).map(String::as_str)
    }

    /// E.164 form of [`lookup_phone_by_name`] when a match exists.
    pub fn lookup_e164_by_name(&self, name: &str) -> Option<String> {
        self.lookup_phone_by_name(name).map(|d| to_e164(&d))
    }

    /// If `name` is blank/unknown and `phone` is in the book, return the display name.
    pub fn enrich_display_name(&self, phone: &str, name: &str) -> Option<String> {
        if !is_blank_or_unknown_name(name) {
            return None;
        }
        self.lookup_name_by_phone(phone).map(str::to_string)
    }

    pub fn len(&self) -> usize {
        self.by_phone.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_phone.is_empty() && self.by_name.is_empty()
    }
}

/// Require exactly one of `--contacts` or `--vcf` and load the book.
pub fn resolve_contacts_cli(
    contacts: Option<PathBuf>,
    vcf: Option<PathBuf>,
) -> Result<(ContactsBook, PathBuf)> {
    match (contacts, vcf) {
        (Some(path), None) => {
            let book = ContactsBook::load_csv(&path)?;
            Ok((book, path))
        }
        (None, Some(path)) => {
            let book = ContactsBook::load_vcf(&path)?;
            Ok((book, path))
        }
        (Some(_), Some(_)) => {
            bail!("pass only one of --contacts PATH.csv or --vcf PATH.vcf")
        }
        (None, None) => {
            bail!(
                "contacts required: pass --contacts PATH.csv (vault-shaped phones,first_name,last_name) \
                 or --vcf PATH.vcf — name/phone resolution happens at export, not in vault csv-ingest"
            )
        }
    }
}

pub(crate) fn split_csv_line(line: &str) -> Vec<String> {
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

fn all_valid_phones(phones_raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    push_phones_from_raw(phones_raw, &mut out);
    out
}

/// Collect sanitized digit strings from semicolon-separated fields and `+E.164` tokens in free text.
fn push_phones_from_raw(raw: &str, out: &mut Vec<String>) {
    for part in raw.split([';', ',', '|']) {
        if let Some(digits) = sanitize_number(part.trim()) {
            if !out.contains(&digits) {
                out.push(digits);
            }
        }
    }
    // Scrape bare +digits runs (PROP-ID notes, trailing phones in Notes blobs).
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i > start + 1 {
                if let Some(digits) = sanitize_number(&raw[start..i]) {
                    if !out.contains(&digits) {
                        out.push(digits);
                    }
                }
            }
        } else {
            i += 1;
        }
    }
}

fn parse_exclude(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_file(dir: &tempfile::TempDir, name: &str, body: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut f = File::create(&path).unwrap();
        write!(f, "{body}").unwrap();
        path
    }

    #[test]
    fn loads_csv_both_directions() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "contacts.csv",
            "phones,first_name,last_name\n\
15555550122,Sam,Example\n\
+15555550133;+15555550144,Pat,Contact\n",
        );
        let book = ContactsBook::load_csv(&path).unwrap();
        assert_eq!(
            book.lookup_phone_by_name("Sam Example").as_deref(),
            Some("5555550122")
        );
        assert_eq!(
            book.lookup_name_by_phone("+15555550122"),
            Some("Sam Example")
        );
        assert_eq!(
            book.lookup_name_by_phone("5555550133"),
            Some("Pat Contact")
        );
        assert_eq!(
            book.lookup_name_by_phone("5555550144"),
            Some("Pat Contact")
        );
    }

    #[test]
    fn skips_excluded_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "contacts.csv",
            "phones,first_name,last_name,exclude\n\
+15555550100,Ada,Lovelace,false\n\
+15555550999,Skip,Me,true\n",
        );
        let book = ContactsBook::load_csv(&path).unwrap();
        assert_eq!(
            book.lookup_phone_by_name("Ada Lovelace").as_deref(),
            Some("5555550100")
        );
        assert!(book.lookup_phone_by_name("Skip Me").is_none());
    }

    #[test]
    fn loads_vcf() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "contacts.vcf",
            "BEGIN:VCARD\nVERSION:3.0\nN:Lovelace;Ada;;;\nFN:Ada Lovelace\n\
TEL;TYPE=CELL:+1-555-555-0100\nEND:VCARD\n",
        );
        let book = ContactsBook::load_vcf(&path).unwrap();
        assert_eq!(
            book.lookup_phone_by_name("Ada Lovelace").as_deref(),
            Some("5555550100")
        );
        assert_eq!(
            book.lookup_name_by_phone("5555550100"),
            Some("Ada Lovelace")
        );
    }

    #[test]
    fn resolve_cli_requires_one() {
        assert!(resolve_contacts_cli(None, None).is_err());
        let dir = tempfile::tempdir().unwrap();
        let csv = write_file(
            &dir,
            "c.csv",
            "phones,first_name,last_name\n+15555550100,A,B\n",
        );
        let vcf = write_file(
            &dir,
            "c.vcf",
            "BEGIN:VCARD\nN:B;A;;;\nTEL:+15555550100\nEND:VCARD\n",
        );
        assert!(resolve_contacts_cli(Some(csv.clone()), Some(vcf)).is_err());
        assert!(resolve_contacts_cli(Some(csv), None).is_ok());
    }

    #[test]
    fn enrich_only_when_blank() {
        let mut book = ContactsBook::empty();
        book.insert_entry("Sam Example", &["5555550122".into()]);
        assert_eq!(
            book.enrich_display_name("5555550122", "").as_deref(),
            Some("Sam Example")
        );
        assert_eq!(
            book.enrich_display_name("5555550122", "Already Set"),
            None
        );
    }

    #[test]
    fn loads_imazing_contacts_phone_cols_and_notes() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "Contacts.csv",
            "First Name,Middle Name,Last Name,Mobile Phone,Home Phone,Notes\n\
Bob,,McRoy,+13212462167,,mcroyr@gmail.com\n\
Kyle,,,,,PROP-ID: +17276875182; \n\
NoPhone,,Person,,,,\n",
        );
        let book = ContactsBook::load_imazing_contacts_csv(&path).unwrap();
        assert_eq!(
            book.lookup_phone_by_name("Bob McRoy").as_deref(),
            Some("3212462167")
        );
        assert_eq!(
            book.lookup_name_by_phone("+13212462167"),
            Some("Bob McRoy")
        );
        assert_eq!(
            book.lookup_phone_by_name("Kyle").as_deref(),
            Some("7276875182")
        );
        assert!(book.lookup_phone_by_name("NoPhone Person").is_none());
    }
}
