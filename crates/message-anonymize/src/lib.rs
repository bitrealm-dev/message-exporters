//! Stable, non-reversible anonymization for exporter CSV output.
//!
//! Fake identities are derived with HMAC-SHA256 over a secret key. The same
//! key always yields the same remaps; fakes do not embed or encrypt the
//! original, and no mapping sidecar is written.

mod names;

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use hmac::{Hmac, Mac};
use rand::RngCore;
use regex::Regex;
use serde_json::{json, Value};
use sha2::Sha256;

use names::{FIRST_NAMES, LAST_NAMES};

type HmacSha256 = Hmac<Sha256>;

const PLACEHOLDER_JPG: &[u8] = include_bytes!("../assets/placeholder.jpg");
const PLACEHOLDER_MP4: &[u8] = include_bytes!("../assets/placeholder.mp4");
const PLACEHOLDER_BIN: &[u8] = include_bytes!("../assets/placeholder.bin");

const REL_IMAGE: &str = "attachments/placeholder.jpg";
const REL_VIDEO: &str = "attachments/placeholder.mp4";
const REL_OTHER: &str = "attachments/placeholder.bin";

/// Media class for placeholder substitution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaClass {
    Image,
    Video,
    Other,
}

/// Keyed anonymizer (in-memory cache only; never writes a real→fake map).
pub struct Anonymizer {
    key: [u8; 32],
    phone_cache: HashMap<String, String>,
    name_cache: HashMap<String, (String, String)>,
    email_cache: HashMap<String, String>,
    text_cache: HashMap<String, String>,
}

impl Anonymizer {
    pub fn new(key: [u8; 32]) -> Self {
        Self {
            key,
            phone_cache: HashMap::new(),
            name_cache: HashMap::new(),
            email_cache: HashMap::new(),
            text_cache: HashMap::new(),
        }
    }

    pub fn key_hex(&self) -> String {
        hex::encode(self.key)
    }

    fn digest(&self, domain: &str, value: &str) -> [u8; 32] {
        let mut mac =
            HmacSha256::new_from_slice(&self.key).expect("HMAC accepts any key length");
        mac.update(domain.as_bytes());
        mac.update(b"\0");
        mac.update(value.as_bytes());
        let result = mac.finalize().into_bytes();
        let mut out = [0u8; 32];
        out.copy_from_slice(&result);
        out
    }

    /// Fake phone with the same digit count as `raw` (formatting stripped for the key).
    /// Preserves a leading `+` when the source had one.
    pub fn anonymize_phone(&mut self, raw: &str) -> String {
        let trimmed = raw.trim();
        let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return trimmed.to_string();
        }
        if let Some(cached) = self.phone_cache.get(&digits) {
            return cached.clone();
        }
        let has_plus = trimmed.starts_with('+');
        let d = self.digest("phone", &digits);
        let mut fake_digits = String::with_capacity(digits.len());
        let mut i = 0usize;
        while fake_digits.len() < digits.len() {
            let b = d[i % d.len()];
            fake_digits.push(char::from(b'0' + (b % 10)));
            i += 1;
        }
        // Avoid all-zeros / all-same when possible.
        if fake_digits.chars().all(|c| c == '0') {
            fake_digits = "1".repeat(digits.len());
        }
        let fake = if has_plus {
            format!("+{fake_digits}")
        } else {
            fake_digits
        };
        self.phone_cache.insert(digits, fake.clone());
        fake
    }

    /// Human display name from the name word lists (keyed by normalized original).
    pub fn anonymize_display_name(&mut self, raw: &str) -> String {
        let key = normalize_name_key(raw);
        if key.is_empty() {
            return String::new();
        }
        let (first, last) = self.name_parts(&key);
        format!("{first} {last}")
    }

    fn name_parts(&mut self, key: &str) -> (String, String) {
        if let Some(cached) = self.name_cache.get(key) {
            return cached.clone();
        }
        let d = self.digest("name", key);
        let fi = u32::from_le_bytes(d[0..4].try_into().unwrap()) as usize % FIRST_NAMES.len();
        let li = u32::from_le_bytes(d[4..8].try_into().unwrap()) as usize % LAST_NAMES.len();
        let pair = (FIRST_NAMES[fi].to_string(), LAST_NAMES[li].to_string());
        self.name_cache.insert(key.to_string(), pair.clone());
        pair
    }

    /// Display name derived from a phone/email handle (keeps person consistent with handle).
    pub fn display_name_for_handle(&mut self, handle: &str) -> String {
        let h = handle.trim();
        if h.is_empty() {
            return String::new();
        }
        if looks_like_email(h) {
            let (first, last) = self.name_parts(&format!("email:{}" , h.to_ascii_lowercase()));
            return format!("{first} {last}");
        }
        let digits: String = h.chars().filter(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            let (first, last) = self.name_parts(&format!("phone:{digits}"));
            return format!("{first} {last}");
        }
        self.anonymize_display_name(h)
    }

    pub fn anonymize_email(&mut self, raw: &str) -> String {
        let key = raw.trim().to_ascii_lowercase();
        if key.is_empty() {
            return String::new();
        }
        if let Some(cached) = self.email_cache.get(&key) {
            return cached.clone();
        }
        let (first, last) = self.name_parts(&format!("email:{key}"));
        let fake = format!(
            "{}.{}@example.invalid",
            first.to_ascii_lowercase(),
            last.to_ascii_lowercase()
        );
        self.email_cache.insert(key, fake.clone());
        fake
    }

    /// Handle: phone, email, or opaque string → fake of the same kind.
    pub fn anonymize_handle(&mut self, raw: &str) -> String {
        let t = raw.trim();
        if t.is_empty() {
            return String::new();
        }
        if looks_like_email(t) {
            return self.anonymize_email(t);
        }
        let digit_count = t.chars().filter(|c| c.is_ascii_digit()).count();
        if digit_count >= 5 {
            return self.anonymize_phone(t);
        }
        // Name-only chat id / opaque handle
        self.anonymize_display_name(t)
    }

    /// Same character length; content from digest-driven filler (not a scramble of the original).
    pub fn anonymize_text(&mut self, raw: &str) -> String {
        if raw.is_empty() {
            return String::new();
        }
        if let Some(cached) = self.text_cache.get(raw) {
            return cached.clone();
        }
        let d = self.digest("text", raw);
        let len = raw.chars().count();
        const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz ";
        let mut out = String::with_capacity(raw.len());
        let mut i = 0usize;
        for ch in raw.chars() {
            if ch == '\n' || ch == '\r' || ch == '\t' {
                out.push(ch);
            } else {
                let b = d[i % d.len()];
                out.push(ALPHABET[(b as usize) % ALPHABET.len()] as char);
                i += 1;
            }
        }
        // Ensure we didn't shrink due to combining edge cases — pad/truncate to char len.
        let mut chars: Vec<char> = out.chars().collect();
        while chars.len() < len {
            chars.push('x');
        }
        chars.truncate(len);
        let fake: String = chars.into_iter().collect();
        self.text_cache.insert(raw.to_string(), fake.clone());
        fake
    }

    /// Remap phone/email substrings inside a free-form field (e.g. Chat Session).
    pub fn anonymize_mixed_field(&mut self, raw: &str) -> String {
        if raw.is_empty() {
            return String::new();
        }
        let email_re = Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").unwrap();
        let phone_re = Regex::new(r"\+?\d[\d\-\s().]{4,}\d").unwrap();

        let mut out = raw.to_string();
        // Emails first so we don't mangle them as phones.
        let emails: Vec<String> = email_re
            .find_iter(&out)
            .map(|m| m.as_str().to_string())
            .collect();
        for e in emails {
            let fake = self.anonymize_email(&e);
            out = out.replacen(&e, &fake, 1);
        }
        let phones: Vec<String> = phone_re
            .find_iter(&out)
            .map(|m| m.as_str().to_string())
            .collect();
        for p in phones {
            let digit_count = p.chars().filter(|c| c.is_ascii_digit()).count();
            if digit_count < 5 {
                continue;
            }
            let fake = self.anonymize_phone(&p);
            out = out.replacen(&p, &fake, 1);
        }
        out
    }
}

fn normalize_name_key(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn looks_like_email(s: &str) -> bool {
    s.contains('@') && s.contains('.')
}

/// Classify attachment by MIME and/or file extension.
pub fn classify_attachment(mime: Option<&str>, path: Option<&str>) -> MediaClass {
    if let Some(m) = mime {
        let m = m.to_ascii_lowercase();
        if m.starts_with("image/") {
            return MediaClass::Image;
        }
        if m.starts_with("video/") {
            return MediaClass::Video;
        }
    }
    if let Some(p) = path {
        let ext = Path::new(p)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" | "bmp" | "tif" | "tiff" => {
                return MediaClass::Image;
            }
            "mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi" | "3gp" => return MediaClass::Video,
            _ => {}
        }
    }
    MediaClass::Other
}

pub fn placeholder_rel_path(class: MediaClass) -> &'static str {
    match class {
        MediaClass::Image => REL_IMAGE,
        MediaClass::Video => REL_VIDEO,
        MediaClass::Other => REL_OTHER,
    }
}

/// Write the three shared placeholder files under `output_dir/attachments/`.
pub fn materialize_placeholders(output_dir: &Path) -> Result<()> {
    let dir = output_dir.join("attachments");
    fs::create_dir_all(&dir)?;
    // Remove prior real media.
    if dir.is_dir() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != "placeholder.jpg"
                    && name != "placeholder.mp4"
                    && name != "placeholder.bin"
                {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }
    fs::write(dir.join("placeholder.jpg"), PLACEHOLDER_JPG)?;
    fs::write(dir.join("placeholder.mp4"), PLACEHOLDER_MP4)?;
    fs::write(dir.join("placeholder.bin"), PLACEHOLDER_BIN)?;
    Ok(())
}

/// Parse `--anonymize-seed` hex or generate a random key; print seed to stderr when generated.
pub fn resolve_anonymizer(seed_hex: Option<&str>) -> Result<Anonymizer> {
    let key = match seed_hex {
        Some(s) => {
            let s = s.trim();
            let bytes = hex::decode(s).context("invalid --anonymize-seed (expected hex)")?;
            if bytes.len() != 32 {
                bail!(
                    "--anonymize-seed must be 32 bytes (64 hex chars), got {} bytes",
                    bytes.len()
                );
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            key
        }
        None => {
            let mut key = [0u8; 32];
            rand::rng().fill_bytes(&mut key);
            let hex_key = hex::encode(key);
            let _ = writeln!(
                std::io::stderr(),
                "anonymize-seed: {hex_key}  (save to reproduce; not written to output)"
            );
            key
        }
    };
    Ok(Anonymizer::new(key))
}

const NEAR_VAULT_IDENTITY_COLS: &[&str] = &[
    "chat_identifier",
    "group_title",
    "participants_json",
    "sender_handle",
    "sender_display_name",
    "contact_name",
    "text",
    "subject",
    "attachments_json",
    "announcement",
    "shared_location",
];

/// Anonymize all `*.csv` in a near-vault export directory and replace attachments.
pub fn anonymize_near_vault_dir(output_dir: &Path, anon: &mut Anonymizer) -> Result<usize> {
    materialize_placeholders(output_dir)?;
    let mut count = 0usize;
    let mut csv_paths: Vec<PathBuf> = fs::read_dir(output_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
        })
        .collect();
    csv_paths.sort();
    for path in csv_paths {
        anonymize_near_vault_csv_file(&path, &path, anon)?;
        count += 1;
    }
    rename_chat_csv_files(output_dir)?;
    Ok(count)
}

fn rename_chat_csv_files(output_dir: &Path) -> Result<()> {
    let mut csv_paths: Vec<PathBuf> = fs::read_dir(output_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
        })
        .collect();
    csv_paths.sort();
    for path in csv_paths {
        let mut rdr = csv::ReaderBuilder::new().flexible(true).from_path(&path)?;
        let headers = rdr.headers()?.clone();
        let Some(chat_idx) = headers.iter().position(|h| h == "chat_identifier") else {
            continue;
        };
        let Some(Ok(first)) = rdr.records().next() else {
            continue;
        };
        let chat_id = first.get(chat_idx).unwrap_or("").trim();
        if chat_id.is_empty() {
            continue;
        }
        let safe: String = chat_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let dest = output_dir.join(format!("{safe}.csv"));
        if dest != path && !dest.exists() {
            let _ = fs::rename(&path, &dest);
        }
    }
    Ok(())
}

fn anonymize_near_vault_csv_file(
    input: &Path,
    output: &Path,
    anon: &mut Anonymizer,
) -> Result<()> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(input)
        .with_context(|| format!("read {}", input.display()))?;
    let headers = rdr.headers()?.clone();
    let mut rows: Vec<csv::StringRecord> = Vec::new();
    for result in rdr.records() {
        let record = result?;
        rows.push(anonymize_near_vault_record(&headers, &record, anon)?);
    }

    let tmp = output.with_extension("csv.tmp");
    {
        let mut wtr = csv::Writer::from_path(&tmp)
            .with_context(|| format!("write {}", tmp.display()))?;
        wtr.write_record(&headers)?;
        for row in &rows {
            wtr.write_record(row)?;
        }
        wtr.flush()?;
    }
    fs::rename(&tmp, output)?;
    Ok(())
}

fn anonymize_near_vault_record(
    headers: &csv::StringRecord,
    record: &csv::StringRecord,
    anon: &mut Anonymizer,
) -> Result<csv::StringRecord> {
    let mut out = csv::StringRecord::new();
    let mut sender_handle_original = String::new();
    for (i, header) in headers.iter().enumerate() {
        let val = record.get(i).unwrap_or("");
        let new_val = match header {
            "chat_identifier" => anon.anonymize_handle(val),
            "group_title" => {
                if val.is_empty() {
                    String::new()
                } else {
                    anon.anonymize_mixed_field(val)
                }
            }
            "participants_json" => anonymize_participants_json(val, anon),
            "sender_handle" => {
                sender_handle_original = val.to_string();
                anon.anonymize_handle(val)
            }
            "sender_display_name" | "contact_name" => {
                if val.is_empty() {
                    String::new()
                } else if !sender_handle_original.is_empty() {
                    anon.display_name_for_handle(&sender_handle_original)
                } else {
                    anon.anonymize_display_name(val)
                }
            }
            "text" | "subject" | "announcement" => anon.anonymize_text(val),
            "attachments_json" => anonymize_attachments_json(val),
            "shared_location" => {
                if val.is_empty() {
                    String::new()
                } else {
                    anon.anonymize_text(val)
                }
            }
            _ => {
                if NEAR_VAULT_IDENTITY_COLS.contains(&header) {
                    anon.anonymize_text(val)
                } else {
                    val.to_string()
                }
            }
        };
        out.push_field(&new_val);
    }
    Ok(out)
}

fn anonymize_participants_json(raw: &str, anon: &mut Anonymizer) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" || trimmed == "[]" {
        return trimmed.to_string();
    }
    let Ok(mut value) = serde_json::from_str::<Value>(trimmed) else {
        return anon.anonymize_mixed_field(raw);
    };
    if let Some(arr) = value.as_array_mut() {
        for item in arr.iter_mut() {
            if let Some(obj) = item.as_object_mut() {
                if let Some(h) = obj.get("handle").and_then(|v| v.as_str()) {
                    let fake_n = anon.display_name_for_handle(h);
                    let fake_h = anon.anonymize_handle(h);
                    obj.insert("handle".into(), json!(fake_h));
                    if obj.contains_key("display_name") {
                        obj.insert("display_name".into(), json!(fake_n));
                    }
                }
            } else if let Some(s) = item.as_str() {
                *item = json!(anon.anonymize_handle(s));
            }
        }
    }
    serde_json::to_string(&value).unwrap_or_else(|_| "[]".into())
}

fn anonymize_attachments_json(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" || trimmed == "[]" {
        return trimmed.to_string();
    }
    let Ok(mut value) = serde_json::from_str::<Value>(trimmed) else {
        return "[]".into();
    };
    if let Some(arr) = value.as_array_mut() {
        for item in arr.iter_mut() {
            if let Some(obj) = item.as_object_mut() {
                let mime = obj
                    .get("mime_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let path = obj
                    .get("path")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let class = classify_attachment(mime.as_deref(), path.as_deref());
                let rel = placeholder_rel_path(class);
                obj.insert("path".into(), json!(rel));
                if let Some(orig) = obj.get("original_name").and_then(|v| v.as_str()) {
                    let ext = Path::new(rel)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("bin");
                    let stem = Path::new(orig)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("file");
                    // Keep extension class only; drop original basename PII lightly.
                    let _ = stem;
                    obj.insert("original_name".into(), json!(format!("attachment.{ext}")));
                }
                if let Some(t) = obj.get_mut("transcription") {
                    if t.as_str().is_some_and(|s| !s.is_empty()) {
                        *t = json!("[redacted]");
                    }
                }
            }
        }
    }
    serde_json::to_string(&value).unwrap_or_else(|_| "[]".into())
}

/// Anonymize iMazing vendor CSV(s) into an output directory.
pub fn anonymize_imazing(input: &Path, output_dir: &Path, anon: &mut Anonymizer) -> Result<usize> {
    fs::create_dir_all(output_dir)?;
    materialize_placeholders(output_dir)?;
    let inputs: Vec<PathBuf> = if input.is_file() {
        vec![input.to_path_buf()]
    } else if input.is_dir() {
        let mut paths: Vec<PathBuf> = fs::read_dir(input)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
            })
            .collect();
        paths.sort();
        paths
    } else {
        bail!("input not found: {}", input.display());
    };
    if inputs.is_empty() {
        bail!("no CSV files found at {}", input.display());
    }
    let mut n = 0usize;
    for src in inputs {
        let dest = output_dir.join(
            src.file_name()
                .context("CSV path missing file name")?,
        );
        anonymize_imazing_csv_file(&src, &dest, anon)?;
        n += 1;
    }
    Ok(n)
}

fn anonymize_imazing_csv_file(input: &Path, output: &Path, anon: &mut Anonymizer) -> Result<()> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(input)
        .with_context(|| format!("read {}", input.display()))?;
    let headers = rdr.headers()?.clone();
    let mut rows = Vec::new();
    for result in rdr.records() {
        let record = result?;
        rows.push(anonymize_imazing_record(&headers, &record, anon));
    }
    let mut wtr = csv::Writer::from_path(output)
        .with_context(|| format!("write {}", output.display()))?;
    wtr.write_record(&headers)?;
    for row in &rows {
        wtr.write_record(row)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Anonymize an iMazing Chat Session value (`Name`, `Name & Name`, or phones).
fn anonymize_imazing_session(raw: &str, anon: &mut Anonymizer) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Prefer phone/email rewrite when present in the whole string.
    let mixed = anon.anonymize_mixed_field(trimmed);
    if mixed != trimmed {
        return mixed;
    }
    // Name-only sessions: split on " & " and remap each display name.
    trimmed
        .split(" & ")
        .map(|part| {
            let p = part.trim();
            if p.is_empty() {
                String::new()
            } else if looks_like_email(p) || p.chars().filter(|c| c.is_ascii_digit()).count() >= 5 {
                anon.anonymize_handle(p)
            } else {
                anon.anonymize_display_name(p)
            }
        })
        .collect::<Vec<_>>()
        .join(" & ")
}

fn anonymize_imazing_record(
    headers: &csv::StringRecord,
    record: &csv::StringRecord,
    anon: &mut Anonymizer,
) -> csv::StringRecord {
    let mut out = csv::StringRecord::new();
    let mut chat_session_original = String::new();
    let mut sender_id_original = String::new();
    for (i, header) in headers.iter().enumerate() {
        let val = record.get(i).unwrap_or("");
        let new_val = match header {
            "Chat Session" => {
                chat_session_original = val.to_string();
                anonymize_imazing_session(val, anon)
            }
            "Replying to" => {
                if val.trim().is_empty() {
                    String::new()
                } else {
                    anonymize_imazing_session(val, anon)
                }
            }
            "Sender ID" => {
                sender_id_original = val.to_string();
                anon.anonymize_handle(val)
            }
            "Sender Name" => {
                if val.is_empty() {
                    String::new()
                } else if !chat_session_original.is_empty()
                    && normalize_name_key(val) == normalize_name_key(&chat_session_original)
                {
                    // Keep chat title and peer name aligned when iMazing used the same string.
                    anon.anonymize_display_name(&chat_session_original)
                } else if !sender_id_original.is_empty() {
                    anon.display_name_for_handle(&sender_id_original)
                } else {
                    anon.anonymize_display_name(val)
                }
            }
            "Text" | "Subject" | "Reactions" => anon.anonymize_text(val),
            "Attachment" => {
                if val.trim().is_empty() {
                    String::new()
                } else {
                    let class = classify_attachment(None, Some(val));
                    // iMazing often stores bare filenames; use basename of placeholder.
                    Path::new(placeholder_rel_path(class))
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("placeholder.bin")
                        .to_string()
                }
            }
            "Attachment type" => val.to_string(),
            _ => val.to_string(),
        };
        out.push_field(&new_val);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(n: u8) -> [u8; 32] {
        [n; 32]
    }

    #[test]
    fn phone_stable_same_key() {
        let mut a = Anonymizer::new(key(1));
        let mut b = Anonymizer::new(key(1));
        assert_eq!(
            a.anonymize_phone("+15555550100"),
            b.anonymize_phone("+15555550100")
        );
    }

    #[test]
    fn phone_differs_other_key() {
        let mut a = Anonymizer::new(key(1));
        let mut b = Anonymizer::new(key(2));
        assert_ne!(
            a.anonymize_phone("+15555550100"),
            b.anonymize_phone("+15555550100")
        );
    }

    #[test]
    fn phone_preserves_digit_length_and_plus() {
        let mut a = Anonymizer::new(key(3));
        let fake = a.anonymize_phone("+15555550100");
        assert!(fake.starts_with('+'));
        assert_eq!(
            fake.chars().filter(|c| c.is_ascii_digit()).count(),
            11
        );
        assert!(!fake.contains("5555550100"));
    }

    #[test]
    fn name_is_human() {
        let mut a = Anonymizer::new(key(4));
        let name = a.anonymize_display_name("Secret Person");
        let parts: Vec<_> = name.split_whitespace().collect();
        assert_eq!(parts.len(), 2);
        assert!(FIRST_NAMES.contains(&parts[0]));
        assert!(LAST_NAMES.contains(&parts[1]));
    }

    #[test]
    fn text_same_length_not_original() {
        let mut a = Anonymizer::new(key(5));
        let src = "Hello, call me at dinner!";
        let fake = a.anonymize_text(src);
        assert_eq!(fake.chars().count(), src.chars().count());
        assert_ne!(fake, src);
        assert!(!fake.contains("dinner"));
    }

    #[test]
    fn near_vault_dir_smoke() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("_15555550100.csv");
        let mut wtr = csv::Writer::from_path(&csv_path).unwrap();
        wtr.write_record([
            "chat_identifier",
            "sender_handle",
            "sender_display_name",
            "text",
            "attachments_json",
            "export_source",
        ])
        .unwrap();
        wtr.write_record([
            "+15555550100",
            "+15555550100",
            "Alice Secret",
            "Meet at 9",
            r#"[{"path":"attachments/photo.jpg","mime_type":"image/jpeg","original_name":"photo.jpg","is_sticker":false}]"#,
            "go-sms-pro",
        ])
        .unwrap();
        wtr.flush().unwrap();
        fs::create_dir_all(dir.path().join("attachments")).unwrap();
        fs::write(dir.path().join("attachments/photo.jpg"), b"REAL").unwrap();

        let mut anon = Anonymizer::new(key(9));
        anonymize_near_vault_dir(dir.path(), &mut anon).unwrap();

        assert!(dir.path().join("attachments/placeholder.jpg").is_file());
        assert!(!dir.path().join("attachments/photo.jpg").exists());

        let mut found_original = false;
        for entry in fs::read_dir(dir.path()).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("csv") {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            if text.contains("15555550100") || text.contains("Alice Secret") || text.contains("Meet at 9")
            {
                found_original = true;
            }
            assert!(text.contains("attachments/placeholder.jpg"));
        }
        assert!(!found_original);
    }
}
