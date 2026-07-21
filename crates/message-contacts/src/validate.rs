//! Copy a contacts VCF/CSV and rewrite phones that [`normalize_certain`] accepts.

use crate::name::collapse_inner_whitespace;
use anyhow::{bail, Context, Result};
use message_phone::{normalize_certain, normalize_uncertain_reason, PhoneRegion};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidateMode {
    /// Analyze only; do not write corrected files or a log file.
    Check,
    /// Write corrected copy (+ VCF for CSV inputs) and log beside the input.
    #[default]
    Update,
}

#[derive(Debug, Default)]
pub struct ValidateReport {
    pub rewritten: u64,
    pub uncertain: u64,
    pub duplicate_groups: u64,
    /// Planned or written primary output path.
    pub output_path: PathBuf,
    /// Companion VCF (CSV inputs only).
    pub vcf_path: Option<PathBuf>,
    /// Planned or written log path.
    pub log_path: PathBuf,
    /// True when files were written (`Update` mode).
    pub wrote_files: bool,
    /// Full validate.log contents (also returned for Check so UIs can display them).
    pub log_lines: Vec<String>,
}

#[derive(Debug, Clone)]
struct OutCard {
    fn_raw: String,
    n_family: String,
    n_given: String,
    phones: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContactsFormat {
    Vcf,
    VaultCsv,
    ImazingCsv,
}

/// Validate contacts beside `input`.
///
/// - [`ValidateMode::Update`]: write `{stem}-corrected-{YYMMDD-hhmmss}.{ext}` (+ `.vcf` for CSV) and `.log`.
/// - [`ValidateMode::Check`]: same analysis; no files written; results are in [`ValidateReport::log_lines`].
pub fn validate_contacts_file(
    input: &Path,
    region: PhoneRegion,
    mode: ValidateMode,
) -> Result<ValidateReport> {
    if !input.is_file() {
        bail!("input not found: {}", input.display());
    }

    let write = mode == ValidateMode::Update;
    let format = detect_format(input)?;
    let (output_path, log_path) = corrected_output_paths(input);

    let mut log_lines: Vec<String> = Vec::new();
    log_lines.push(format!(
        "# contacts-validate mode={} region={region}",
        match mode {
            ValidateMode::Check => "check",
            ValidateMode::Update => "update",
        }
    ));
    log_lines.push(format!("# input={}", input.display()));
    log_lines.push(format!(
        "# output={}{}",
        output_path.display(),
        if write { "" } else { " (not written)" }
    ));
    log_lines.push(String::new());

    let mut rewritten = 0u64;
    let mut uncertain = 0u64;
    // e164 → list of contact display names that own it (after rewrite)
    let mut by_e164: HashMap<String, Vec<String>> = HashMap::new();
    let mut cards: Vec<OutCard> = Vec::new();

    match format {
        ContactsFormat::Vcf => {
            rewrite_vcf(
                input,
                &output_path,
                region,
                write,
                &mut rewritten,
                &mut uncertain,
                &mut log_lines,
                &mut by_e164,
            )?;
        }
        ContactsFormat::VaultCsv => {
            rewrite_vault_csv(
                input,
                &output_path,
                region,
                write,
                &mut rewritten,
                &mut uncertain,
                &mut log_lines,
                &mut by_e164,
                &mut cards,
            )?;
        }
        ContactsFormat::ImazingCsv => {
            rewrite_imazing_csv(
                input,
                &output_path,
                region,
                write,
                &mut rewritten,
                &mut uncertain,
                &mut log_lines,
                &mut by_e164,
                &mut cards,
            )?;
        }
    }

    let vcf_path = if matches!(
        format,
        ContactsFormat::VaultCsv | ContactsFormat::ImazingCsv
    ) {
        let vcf_path = output_path.with_extension("vcf");
        if write {
            write_vcf_cards(&vcf_path, &cards)?;
        }
        log_lines.push(format!(
            "# vcf={}{}",
            vcf_path.display(),
            if write { "" } else { " (not written)" }
        ));
        Some(vcf_path)
    } else {
        None
    };

    let mut duplicate_groups = 0u64;
    log_lines.push(String::new());
    log_lines.push("# duplicate numbers (same E.164 on more than one contact)".into());
    let mut keys: Vec<_> = by_e164.keys().cloned().collect();
    keys.sort();
    for e164 in keys {
        let names = by_e164.get(&e164).cloned().unwrap_or_default();
        if names.len() > 1 {
            duplicate_groups += 1;
            log_lines.push(format!("DUPLICATE {e164}: {}", names.join(" | ")));
        }
    }
    if duplicate_groups == 0 {
        log_lines.push("(none)".into());
    }

    log_lines.push(String::new());
    log_lines.push(format!(
        "# summary rewritten={rewritten} uncertain={uncertain} duplicate_groups={duplicate_groups}"
    ));
    if !write {
        log_lines.push("# check only — no files written".into());
    }

    if write {
        let mut log_file =
            File::create(&log_path).with_context(|| format!("create {}", log_path.display()))?;
        for line in &log_lines {
            writeln!(log_file, "{line}")?;
        }
    }

    Ok(ValidateReport {
        rewritten,
        uncertain,
        duplicate_groups,
        output_path,
        vcf_path,
        log_path,
        wrote_files: write,
        log_lines,
    })
}

fn corrected_output_paths(input: &Path) -> (PathBuf, PathBuf) {
    let parent = input.parent().filter(|p| !p.as_os_str().is_empty());
    let parent = parent.unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("contacts");
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("csv");
    let stamp = chrono::Local::now().format("%y%m%d-%H%M%S");
    let base = format!("{stem}-corrected-{stamp}");
    (parent.join(format!("{base}.{ext}")), parent.join(format!("{base}.log")))
}

fn detect_format(path: &Path) -> Result<ContactsFormat> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "vcf" || ext == "vcard" {
        return Ok(ContactsFormat::Vcf);
    }
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut header = String::new();
    reader.read_line(&mut header)?;
    let header = header.trim_start_matches('\u{feff}').to_ascii_lowercase();
    if header.contains("phones") && header.contains("first_name") {
        return Ok(ContactsFormat::VaultCsv);
    }
    if header.contains("first name") || header.contains("mobile phone") {
        return Ok(ContactsFormat::ImazingCsv);
    }
    if header.contains("begin:vcard") {
        return Ok(ContactsFormat::Vcf);
    }
    bail!(
        "unrecognized contacts format for {} (need .vcf, vault phones/first_name CSV, or iMazing Contacts CSV)",
        path.display()
    );
}

fn rewrite_phone_token(
    raw: &str,
    contact: &str,
    region: PhoneRegion,
    rewritten: &mut u64,
    uncertain: &mut u64,
    log_lines: &mut Vec<String>,
    by_e164: &mut HashMap<String, Vec<String>>,
) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return raw.to_string();
    }
    match normalize_certain(trimmed, region) {
        Some(e164) => {
            *rewritten += 1;
            by_e164
                .entry(e164.clone())
                .or_default()
                .push(contact.to_string());
            e164
        }
        None => {
            *uncertain += 1;
            let reason = normalize_uncertain_reason(trimmed, region);
            log_lines.push(format!(
                "UNCERTAIN contact={contact:?} phone={trimmed:?} reason={reason}"
            ));
            raw.to_string()
        }
    }
}

fn rewrite_phone_list(
    raw: &str,
    contact: &str,
    region: PhoneRegion,
    sep: char,
    rewritten: &mut u64,
    uncertain: &mut u64,
    log_lines: &mut Vec<String>,
    by_e164: &mut HashMap<String, Vec<String>>,
) -> String {
    if raw.trim().is_empty() {
        return raw.to_string();
    }
    let parts: Vec<String> = raw
        .split(sep)
        .map(|p| {
            rewrite_phone_token(
                p,
                contact,
                region,
                rewritten,
                uncertain,
                log_lines,
                by_e164,
            )
        })
        .collect();
    parts.join(&sep.to_string())
}

fn rewrite_vcf(
    input: &Path,
    output: &Path,
    region: PhoneRegion,
    write: bool,
    rewritten: &mut u64,
    uncertain: &mut u64,
    log_lines: &mut Vec<String>,
    by_e164: &mut HashMap<String, Vec<String>>,
) -> Result<()> {
    let text = fs::read_to_string(input).with_context(|| format!("read {}", input.display()))?;
    let mut out = String::new();
    let mut current_name = String::from("(unnamed)");
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.to_ascii_uppercase().starts_with("FN:") {
            current_name = trimmed[3..].trim().to_string();
            if current_name.is_empty() {
                current_name = "(unnamed)".into();
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if trimmed.to_ascii_uppercase().starts_with("N:") && current_name == "(unnamed)" {
            // fallback name from N if FN missing
            let n = &trimmed[2..];
            let parts: Vec<&str> = n.split(';').collect();
            let family = parts.first().copied().unwrap_or("").trim();
            let given = parts.get(1).copied().unwrap_or("").trim();
            current_name = collapse_inner_whitespace(&format!("{given} {family}"));
            if current_name.is_empty() {
                current_name = "(unnamed)".into();
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // TEL;TYPE=CELL:+1-555… or TEL:+1…
        let upper = trimmed.to_ascii_uppercase();
        if upper.starts_with("TEL") {
            if let Some((prefix, value)) = trimmed.split_once(':') {
                let new_val = rewrite_phone_token(
                    value,
                    &current_name,
                    region,
                    rewritten,
                    uncertain,
                    log_lines,
                    by_e164,
                );
                out.push_str(prefix);
                out.push(':');
                out.push_str(&new_val);
                out.push('\n');
                continue;
            }
        }
        if trimmed.eq_ignore_ascii_case("BEGIN:VCARD") {
            current_name = "(unnamed)".into();
        }
        out.push_str(line);
        out.push('\n');
    }
    if write {
        fs::write(output, out).with_context(|| format!("write {}", output.display()))?;
    }
    Ok(())
}

fn write_vcf_cards(path: &Path, cards: &[OutCard]) -> Result<()> {
    let mut out = String::new();
    for card in cards {
        if card.phones.is_empty() && card.fn_raw.is_empty() {
            continue;
        }
        out.push_str("BEGIN:VCARD\n");
        out.push_str("VERSION:3.0\n");
        out.push_str(&format!(
            "N:{};{};;;\n",
            vcf_escape(&card.n_family),
            vcf_escape(&card.n_given)
        ));
        if !card.fn_raw.is_empty() {
            out.push_str(&format!("FN:{}\n", vcf_escape(&card.fn_raw)));
        }
        for phone in &card.phones {
            if !phone.trim().is_empty() {
                out.push_str(&format!("TEL:{}\n", phone.trim()));
            }
        }
        out.push_str("END:VCARD\n");
    }
    fs::write(path, out).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn vcf_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn rewrite_vault_csv(
    input: &Path,
    output: &Path,
    region: PhoneRegion,
    write: bool,
    rewritten: &mut u64,
    uncertain: &mut u64,
    log_lines: &mut Vec<String>,
    by_e164: &mut HashMap<String, Vec<String>>,
    cards: &mut Vec<OutCard>,
) -> Result<()> {
    let file = File::open(input).with_context(|| format!("open {}", input.display()))?;
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(file);
    let headers = rdr.headers()?.clone();
    let header_l: Vec<String> = headers
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let phones_i = header_l
        .iter()
        .position(|h| h == "phones")
        .context("vault CSV missing phones column")?;
    let first_i = header_l.iter().position(|h| h == "first_name");
    let last_i = header_l.iter().position(|h| h == "last_name");

    let mut wtr = if write {
        let out_file =
            File::create(output).with_context(|| format!("create {}", output.display()))?;
        let mut wtr = csv::Writer::from_writer(out_file);
        wtr.write_record(&headers)?;
        Some(wtr)
    } else {
        None
    };

    for (idx, rec) in rdr.records().enumerate() {
        let rec = rec.with_context(|| format!("row {}", idx + 2))?;
        let first = first_i
            .and_then(|i| rec.get(i))
            .unwrap_or("")
            .trim();
        let last = last_i.and_then(|i| rec.get(i)).unwrap_or("").trim();
        let contact = collapse_inner_whitespace(&format!("{first} {last}"));
        let contact = if contact.is_empty() {
            format!("row {}", idx + 2)
        } else {
            contact
        };

        let mut fields: Vec<String> = rec.iter().map(|s| s.to_string()).collect();
        while fields.len() < headers.len() {
            fields.push(String::new());
        }
        if let Some(cell) = fields.get_mut(phones_i) {
            *cell = rewrite_phone_list(
                cell,
                &contact,
                region,
                ';',
                rewritten,
                uncertain,
                log_lines,
                by_e164,
            );
        }
        let phones: Vec<String> = fields
            .get(phones_i)
            .map(|c| {
                c.split(';')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if !phones.is_empty() || !first.is_empty() || !last.is_empty() {
            cards.push(OutCard {
                fn_raw: collapse_inner_whitespace(&format!("{first} {last}")),
                n_family: last.to_string(),
                n_given: first.to_string(),
                phones,
            });
        }
        if let Some(wtr) = wtr.as_mut() {
            wtr.write_record(&fields)?;
        }
    }
    if let Some(mut wtr) = wtr {
        wtr.flush()?;
    }
    Ok(())
}

fn rewrite_imazing_csv(
    input: &Path,
    output: &Path,
    region: PhoneRegion,
    write: bool,
    rewritten: &mut u64,
    uncertain: &mut u64,
    log_lines: &mut Vec<String>,
    by_e164: &mut HashMap<String, Vec<String>>,
    cards: &mut Vec<OutCard>,
) -> Result<()> {
    let file = File::open(input).with_context(|| format!("open {}", input.display()))?;
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(file);
    let headers = rdr.headers()?.clone();
    let header_l: Vec<String> = headers
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();

    let first_i = header_l.iter().position(|h| h == "first name");
    let middle_i = header_l.iter().position(|h| h == "middle name");
    let last_i = header_l.iter().position(|h| h == "last name");
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
    .filter_map(|name| header_l.iter().position(|h| h == *name))
    .collect();

    let mut wtr = if write {
        let out_file =
            File::create(output).with_context(|| format!("create {}", output.display()))?;
        let mut wtr = csv::Writer::from_writer(out_file);
        wtr.write_record(&headers)?;
        Some(wtr)
    } else {
        None
    };

    for (idx, rec) in rdr.records().enumerate() {
        let rec = rec.with_context(|| format!("row {}", idx + 2))?;
        let first = first_i.and_then(|i| rec.get(i)).unwrap_or("").trim();
        let middle = middle_i.and_then(|i| rec.get(i)).unwrap_or("").trim();
        let last = last_i.and_then(|i| rec.get(i)).unwrap_or("").trim();
        let mut parts = Vec::new();
        if !first.is_empty() {
            parts.push(first);
        }
        if !middle.is_empty() {
            parts.push(middle);
        }
        if !last.is_empty() {
            parts.push(last);
        }
        let contact = collapse_inner_whitespace(&parts.join(" "));
        let contact = if contact.is_empty() {
            format!("row {}", idx + 2)
        } else {
            contact
        };

        let mut fields: Vec<String> = rec.iter().map(|s| s.to_string()).collect();
        while fields.len() < headers.len() {
            fields.push(String::new());
        }
        let mut phones = Vec::new();
        for &i in &phone_cols {
            if let Some(cell) = fields.get_mut(i) {
                if cell.trim().is_empty() {
                    continue;
                }
                // iMazing sometimes packs multiple phones with `;`
                *cell = rewrite_phone_list(
                    cell,
                    &contact,
                    region,
                    ';',
                    rewritten,
                    uncertain,
                    log_lines,
                    by_e164,
                );
                for p in cell.split(';') {
                    let p = p.trim();
                    if !p.is_empty() && !phones.iter().any(|x| x == p) {
                        phones.push(p.to_string());
                    }
                }
            }
        }
        if !phones.is_empty() || !contact.is_empty() {
            cards.push(OutCard {
                fn_raw: contact.clone(),
                n_family: last.to_string(),
                n_given: first.to_string(),
                phones,
            });
        }
        if let Some(wtr) = wtr.as_mut() {
            wtr.write_record(&fields)?;
        }
    }
    if let Some(mut wtr) = wtr {
        wtr.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(dir: &tempfile::TempDir, name: &str, body: &str) -> PathBuf {
        let path = dir.path().join(name);
        let mut f = File::create(&path).unwrap();
        write!(f, "{body}").unwrap();
        path
    }

    #[test]
    fn validates_vault_csv_usa() {
        let dir = tempfile::tempdir().unwrap();
        let input = write(
            &dir,
            "contacts.csv",
            "phones,first_name,last_name\n\
(542).341-2398,Ada,Lovelace\n\
1555-4567,Short,Number\n\
(542).341-2398,Clone,Contact\n",
        );
        let report =
            validate_contacts_file(&input, PhoneRegion::Usa, ValidateMode::Update).unwrap();
        assert!(report.wrote_files);
        assert_eq!(report.rewritten, 2);
        assert_eq!(report.uncertain, 1);
        assert_eq!(report.duplicate_groups, 1);
        let name = report.output_path.file_name().unwrap().to_str().unwrap();
        assert!(
            name.starts_with("contacts-corrected-") && name.ends_with(".csv"),
            "unexpected name {name}"
        );
        assert_eq!(report.output_path.parent(), Some(dir.path()));
        assert_eq!(
            report.log_path.extension().and_then(|e| e.to_str()),
            Some("log")
        );
        let body = fs::read_to_string(&report.output_path).unwrap();
        assert!(body.contains("+15423412398"));
        assert!(body.contains("1555-4567"));
        let vcf_path = report.vcf_path.expect("csv should also write vcf");
        assert!(vcf_path.extension().is_some_and(|e| e == "vcf"));
        let vcf = fs::read_to_string(&vcf_path).unwrap();
        assert!(vcf.contains("BEGIN:VCARD"));
        assert!(vcf.contains("FN:Ada Lovelace"));
        assert!(vcf.contains("TEL:+15423412398"));
        let log = fs::read_to_string(&report.log_path).unwrap();
        assert!(log.contains("UNCERTAIN"));
        assert!(log.contains("DUPLICATE"));
    }

    #[test]
    fn check_does_not_write_files() {
        let dir = tempfile::tempdir().unwrap();
        let input = write(
            &dir,
            "contacts.csv",
            "phones,first_name,last_name\n(542).341-2398,Ada,Lovelace\n1555-4567,Short,Number\n",
        );
        let before: Vec<_> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().path()).collect();
        let report =
            validate_contacts_file(&input, PhoneRegion::Usa, ValidateMode::Check).unwrap();
        assert!(!report.wrote_files);
        assert_eq!(report.rewritten, 1);
        assert_eq!(report.uncertain, 1);
        assert!(report.log_lines.iter().any(|l| l.contains("UNCERTAIN")));
        assert!(report
            .log_lines
            .iter()
            .any(|l| l.contains("check only — no files written")));
        let after: Vec<_> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(before.len(), after.len(), "check must not create files");
        assert!(!report.output_path.exists());
    }

    #[test]
    fn validates_vcf_international() {
        let dir = tempfile::tempdir().unwrap();
        let input = write(
            &dir,
            "c.vcf",
            "BEGIN:VCARD\nVERSION:3.0\nFN:Ada Lovelace\n\
TEL;TYPE=CELL:+44 20 7183 8750\n\
TEL;TYPE=HOME:(542).341-2398\n\
END:VCARD\n",
        );
        let report =
            validate_contacts_file(&input, PhoneRegion::International, ValidateMode::Update)
                .unwrap();
        assert_eq!(report.rewritten, 1);
        assert_eq!(report.uncertain, 1);
        let name = report.output_path.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("c-corrected-") && name.ends_with(".vcf"));
        let body = fs::read_to_string(&report.output_path).unwrap();
        assert!(body.contains("+442071838750"));
        assert!(body.contains("(542).341-2398"));
    }
}
