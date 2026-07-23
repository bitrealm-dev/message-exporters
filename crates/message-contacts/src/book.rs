//! Bidirectional contacts index (name↔phone).

use crate::name::{collapse_inner_whitespace, is_blank_or_unknown_name, normalize_name_key};
use crate::vcf::{self, strip_tags};
use anyhow::{bail, Context, Result};
use message_phone::{sanitize_number, to_e164};
use std::collections::HashMap;
use std::fs::File;
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

    /// Load a contacts file using the same format rules as contacts-validate.
    pub fn load_contacts_file(path: &Path) -> Result<Self> {
        use crate::validate::{detect_contacts_format, ContactsFormat};
        let format = detect_contacts_format(path).map_err(|e| {
            if e.details.is_empty() {
                anyhow::anyhow!("{}", e.message)
            } else {
                anyhow::anyhow!("{} ({})", e.message, e.details.join("; "))
            }
        })?;
        match format {
            ContactsFormat::Vcf => Self::load_vcf(path),
            ContactsFormat::ImazingCsv => Self::load_imazing_contacts_csv(path),
        }
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

/// Load contacts from at most one of `--contacts` or `--vcf`.
///
/// `--contacts` accepts the same files as contacts-validate (VCF or iMazing
/// Contacts CSV). `--vcf` is a VCF-only alias.
///
/// When neither is passed, returns an empty book and prints a stderr warning.
pub fn resolve_contacts_cli(
    contacts: Option<PathBuf>,
    vcf: Option<PathBuf>,
) -> Result<(ContactsBook, Option<PathBuf>)> {
    match (contacts, vcf) {
        (Some(path), None) => {
            let book = ContactsBook::load_contacts_file(&path)?;
            Ok((book, Some(path)))
        }
        (None, Some(path)) => {
            let book = ContactsBook::load_contacts_file(&path)?;
            Ok((book, Some(path)))
        }
        (Some(_), Some(_)) => {
            bail!("pass only one of --contacts PATH or --vcf PATH")
        }
        (None, None) => {
            eprintln!(
                "warning: no contacts file provided (--contacts or --vcf); \
                 phone numbers will not be resolved to names"
            );
            Ok((ContactsBook::empty(), None))
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
    fn loads_imazing_csv_both_directions() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "contacts.csv",
            "First Name,Last Name,Mobile Phone,Home Phone\n\
Sam,Example,15555550122,\n\
Pat,Contact,+15555550133,+15555550144\n",
        );
        let book = ContactsBook::load_imazing_contacts_csv(&path).unwrap();
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
    fn rejects_legacy_vault_csv() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "contacts.csv",
            "phones,first_name,last_name\n+15555550100,Ada,Lovelace\n",
        );
        let err = ContactsBook::load_contacts_file(&path).unwrap_err();
        assert!(err.to_string().contains("legacy vault CSV"));
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
    fn resolve_cli_allows_none_and_rejects_both() {
        let (book, path) = resolve_contacts_cli(None, None).unwrap();
        assert!(book.is_empty());
        assert!(path.is_none());
        let dir = tempfile::tempdir().unwrap();
        let csv = write_file(
            &dir,
            "c.csv",
            "First Name,Last Name,Mobile Phone\nA,B,+15555550100\n",
        );
        let vcf = write_file(
            &dir,
            "c.vcf",
            "BEGIN:VCARD\nN:B;A;;;\nTEL:+15555550100\nEND:VCARD\n",
        );
        assert!(resolve_contacts_cli(Some(csv.clone()), Some(vcf)).is_err());
        let (book, path) = resolve_contacts_cli(Some(csv), None).unwrap();
        assert!(!book.is_empty());
        assert!(path.is_some());
    }

    #[test]
    fn resolve_cli_loads_imazing_csv_via_contacts() {
        let dir = tempfile::tempdir().unwrap();
        let csv = write_file(
            &dir,
            "Contacts.csv",
            "First Name,Last Name,Mobile Phone\n\
Ada,Lovelace,+15555550100\n",
        );
        let (book, path) = resolve_contacts_cli(Some(csv), None).unwrap();
        assert!(path.is_some());
        assert_eq!(
            book.lookup_name_by_phone("+15555550100"),
            Some("Ada Lovelace")
        );
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
