//! Parse GO SMS Pro MMS PDU backup files (`I_<timestamp>_*.pdu`).
//! PDU attachment / text extraction heuristics.

use crate::emoji::decode_gosms_emojis;
use std::collections::HashSet;

use crate::phone::sanitize_number;
use anyhow::{Context, Result};
use regex::bytes::Regex as BytesRegex;
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

static PDU_FILENAME_RE: OnceLock<Regex> = OnceLock::new();
static PLMN_RE: OnceLock<BytesRegex> = OnceLock::new();
static SMIL_IMG_SRC_RE: OnceLock<BytesRegex> = OnceLock::new();
static TEXT_CONTENT_RE: OnceLock<BytesRegex> = OnceLock::new();
static MMS_PART_JUNK_RE: OnceLock<Regex> = OnceLock::new();
static PRINTABLE_RUN_RE: OnceLock<BytesRegex> = OnceLock::new();
static TRAILING_GARBAGE_RE: OnceLock<Regex> = OnceLock::new();

const TEXT_PART_END_MARKERS: &[&[u8]] = &[
    b"\x8c",
    b"\xa0\x85",
    b"\x00\x85IMG",
    b"\x85IMG",
    b"\xff\xd8\xff",
    b"\x00\x8e",
    b"\x00\x85",
];

const ATTACHMENT_MAGICS: &[(&[u8], &str)] = &[
    (b"\xff\xd8\xff", ".jpg"),
    (b"\x89PNG\r\n\x1a\n", ".png"),
    (b"GIF87a", ".gif"),
    (b"GIF89a", ".gif"),
    (b"\x00\x00\x00\x18ftyp3gp", ".3gp"),
    (b"ftypmp42", ".mp4"),
    (b"#!AMR", ".amr"),
    (b"RIFF", ".wav"),
];

#[derive(Debug, Clone)]
pub struct ParsedAttachment {
    pub ext: String,
    pub data: Vec<u8>,
    pub smil_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedPdu {
    pub path: std::path::PathBuf,
    pub timestamp: i64,
    pub participants: Vec<String>,
    pub body: String,
    pub attachments: Vec<ParsedAttachment>,
    pub is_sent: bool,
    pub is_group: bool,
    pub sender_number: String,
}

fn timestamp_from_filename(name: &str) -> Option<i64> {
    let re = PDU_FILENAME_RE.get_or_init(|| Regex::new(r"^I_(?P<ts>\d+)_").expect("pdu name"));
    re.captures(name)
        .and_then(|c| c.name("ts"))
        .and_then(|m| m.as_str().parse().ok())
}

pub fn extract_plmn_numbers(data: &[u8]) -> Vec<String> {
    let re = PLMN_RE.get_or_init(|| BytesRegex::new(r"\+(\d{10,15})/TYPE=PLMN").expect("plmn"));
    let mut seen = std::collections::HashSet::new();
    let mut numbers = Vec::new();
    for caps in re.captures_iter(data) {
        let digits = String::from_utf8_lossy(&caps[1]).into_owned();
        if seen.insert(digits.clone()) {
            numbers.push(digits);
        }
    }
    numbers
}

fn extract_smil_image_names(data: &[u8]) -> Vec<String> {
    let re = SMIL_IMG_SRC_RE
        .get_or_init(|| BytesRegex::new(r#"(?i)<img\s+src="([^"]+)""#).expect("smil"));
    let mut seen = std::collections::HashSet::new();
    let mut names = Vec::new();
    for caps in re.captures_iter(data) {
        let name = String::from_utf8_lossy(&caps[1]).into_owned();
        if !name.is_empty() && seen.insert(name.clone()) {
            names.push(name);
        }
    }
    names
}

fn truncate_mms_binary_tail(text: &str) -> String {
    let mut text = text.to_string();
    if let Some(img_idx) = text.find("IMG_") {
        if img_idx > 0 {
            text.truncate(img_idx);
        }
    }
    let trailing = TRAILING_GARBAGE_RE
        .get_or_init(|| Regex::new(r"^(.+!!)[^\w\s]{0,12}$").expect("trail"));
    if let Some(caps) = trailing.captures(&text) {
        text = caps[1].to_string();
    }
    for (index, ch) in text.char_indices() {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            continue;
        }
        let code = ch as u32;
        if code < 32 || code == 127 {
            return text[..index].trim_end().to_string();
        }
    }
    text.trim().to_string()
}

fn is_mms_part_junk(text: &str) -> bool {
    let re = MMS_PART_JUNK_RE.get_or_init(|| {
        Regex::new(
            r#"(?i)^(?:text_\d+\.txt|"?<text_\d+>?|"<\d+>|"<text_\d+\.txt>|IMG_\d+\.[A-Za-z]{3,4})$"#,
        )
        .expect("junk")
    });
    re.is_match(text)
}

fn extract_text_after_marker(data: &[u8], start: usize) -> String {
    let mut end = data.len();
    for sep in TEXT_PART_END_MARKERS {
        if let Some(pos) = find_bytes(data, sep, start) {
            end = end.min(pos);
        }
    }
    let text = String::from_utf8_lossy(&data[start..end])
        .replace('\0', "")
        .trim()
        .to_string();
    let text = truncate_mms_binary_tail(&text);
    if !text.is_empty() && !is_mms_part_junk(&text) {
        decode_gosms_emojis(&text)
    } else {
        String::new()
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    haystack[start..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| start + p)
}

pub fn extract_wap_text_body(data: &[u8]) -> String {
    let re =
        TEXT_CONTENT_RE.get_or_init(|| BytesRegex::new(r"(?-u)\x8etext(?:_\d+)?\.txt\x00").expect("txt"));
    let mut texts = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for m in re.find_iter(data) {
        let text = extract_text_after_marker(data, m.end());
        if !text.is_empty() && seen.insert(text.clone()) {
            texts.push(text);
        }
    }
    if !texts.is_empty() {
        return decode_gosms_emojis(&texts.join("\n"));
    }

    if let Some(smil_end) = find_bytes(data, b"</smil>", 0) {
        let tail = &data[smil_end + 7..];
        let run_re = PRINTABLE_RUN_RE
            .get_or_init(|| BytesRegex::new(r"(?-u)[\x20-\x7e\n\r\t]{8,}").expect("run"));
        if let Some(m) = run_re.find(tail) {
            let text = String::from_utf8_lossy(m.as_bytes()).trim().to_string();
            if !text.is_empty() && !text.starts_with('<') && !is_mms_part_junk(&text) {
                return decode_gosms_emojis(&text);
            }
        }
    }
    String::new()
}

pub fn detect_attachment_blobs(data: &[u8]) -> Vec<(String, usize, usize)> {
    if data.len() < 32 {
        return Vec::new();
    }
    let mut hits: Vec<(usize, &str)> = Vec::new();
    for &(sig, ext) in ATTACHMENT_MAGICS {
        let mut start = 0;
        while let Some(rel) = find_bytes(data, sig, start) {
            hits.push((rel, ext));
            start = rel + 1;
        }
    }
    if hits.is_empty() {
        return Vec::new();
    }
    hits.sort_by_key(|(idx, _)| *idx);
    let mut merged = Vec::new();
    for (idx, (start, ext)) in hits.iter().enumerate() {
        let next_start = hits
            .get(idx + 1)
            .map(|(s, _)| *s)
            .unwrap_or(data.len());
        let size = next_start - start;
        if size < 64 && matches!(*ext, ".jpg" | ".png" | ".gif") {
            continue;
        }
        if *ext == ".wav" && size < 10000 {
            continue;
        }
        merged.push((ext.to_string(), *start, next_start));
    }
    merged
}

fn unique_participants(parts: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for p in parts {
        if seen.insert(p.clone()) {
            unique.push(p.clone());
        }
    }
    unique
}

fn is_owner_digit(digits: &str, owners: &HashSet<String>) -> bool {
    owners.contains(&sanitize_number(digits))
}

fn plmn_address_roles(
    data: &[u8],
    owners: &HashSet<String>,
) -> (Option<String>, bool, bool) {
    let re = PLMN_RE.get_or_init(|| BytesRegex::new(r"\+(\d{10,15})/TYPE=PLMN").expect("plmn"));
    let mut from_digits = None;
    let mut my_is_from = false;
    let mut my_is_to = false;

    for caps in re.captures_iter(data) {
        let digits_raw = String::from_utf8_lossy(&caps[1]).into_owned();
        let normalized = sanitize_number(&digits_raw);
        let pos = caps.get(0).map(|m| m.start()).unwrap_or(0);
        let before_start = pos.saturating_sub(8);
        let before = &data[before_start..pos];
        let before_short_start = pos.saturating_sub(6);
        let before_short = &data[before_short_start..pos];
        let before4 = &data[pos.saturating_sub(4)..pos];

        let has_from_marker = before_short.ends_with(b"\x89\x1a\x80\x18\xea")
            || before_short.ends_with(b"\x8e\x89\x1a\x80\x18\xea");
        let is_to = before_short.ends_with(b"\x97\x18\xea") || before4.ends_with(b"\x97\x18\xea");
        let is_real_from = has_from_marker && !before.contains(&0xa5);

        if is_real_from {
            from_digits = Some(normalized.clone());
            if is_owner_digit(&normalized, owners) {
                my_is_from = true;
            }
        }
        if is_to && is_owner_digit(&normalized, owners) {
            my_is_to = true;
        }
    }

    (from_digits, my_is_from, my_is_to)
}

fn infer_pdu_direction(
    data: &[u8],
    unique_parts: &[String],
    owners: &HashSet<String>,
    primary_digits: &str,
) -> (bool, String) {
    if unique_parts.is_empty() {
        return (false, "Unknown".to_string());
    }

    if unique_parts.len() >= 3 {
        let (from_digits, my_is_from, my_is_to) = plmn_address_roles(data, owners);
        if my_is_from {
            return (true, primary_digits.to_string());
        }
        if let Some(from) = from_digits {
            if !is_owner_digit(&from, owners) {
                return (false, from);
            }
        }
        if my_is_to {
            return (true, primary_digits.to_string());
        }
        let sender = unique_parts[0].clone();
        return (is_owner_digit(&sender, owners), sender);
    }

    let first = unique_parts[0].clone();
    if unique_parts.len() == 2 && is_owner_digit(&unique_parts[1], owners) {
        let re = PLMN_RE.get_or_init(|| BytesRegex::new(r"\+(\d{10,15})/TYPE=PLMN").expect("plmn"));
        if let Some(m) = re.find(data) {
            let before = &data[m.start().saturating_sub(6)..m.start()];
            if before.ends_with(b"\x8e\x89\x1a\x80\x18\xea") {
                return (false, first);
            }
        }
        return (true, primary_digits.to_string());
    }

    if unique_parts.iter().any(|p| is_owner_digit(p, owners)) {
        return (true, primary_digits.to_string());
    }

    (false, first)
}

/// Parse one PDU file. Returns `None` for unparseable / bad filenames.
pub fn parse_pdu_file(path: &Path, owners: &HashSet<String>, primary_digits: &str) -> Result<Option<ParsedPdu>> {
    let data = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    if data.len() < 10 {
        return Ok(None);
    }
    let Some(ts) = timestamp_from_filename(
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(""),
    ) else {
        return Ok(None);
    };

    let participants_raw = extract_plmn_numbers(&data);
    let body = extract_wap_text_body(&data);
    let blobs = detect_attachment_blobs(&data);
    let smil_names = extract_smil_image_names(&data);
    let mut attachments = Vec::new();
    for (i, (ext, start, end)) in blobs.into_iter().enumerate() {
        let smil_name = smil_names.get(i).cloned();
        attachments.push(ParsedAttachment {
            ext,
            data: data[start..end].to_vec(),
            smil_name,
        });
    }

    let normalized_parts: Vec<String> = participants_raw
        .iter()
        .map(|p| sanitize_number(p))
        .collect();
    let unique_parts = unique_participants(&normalized_parts);
    let is_group = unique_parts.len() >= 3;
    let (is_sent, sender_number) =
        infer_pdu_direction(&data, &unique_parts, owners, primary_digits);

    Ok(Some(ParsedPdu {
        path: path.to_path_buf(),
        timestamp: ts,
        participants: unique_parts,
        body,
        attachments,
        is_sent,
        is_group,
        sender_number,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pdu")
            .join(name)
    }

    fn test_owners() -> (HashSet<String>, String) {
        let primary = "5555550100".to_string();
        let mut owners = HashSet::new();
        owners.insert(primary.clone());
        (owners, primary)
    }

    #[test]
    fn invalid_filename_returns_none() {
        let (owners, primary) = test_owners();
        let r = parse_pdu_file(&fixture("bad_name.pdu"), &owners, &primary).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn received_one_to_one() {
        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&fixture("I_1609459200_recv.pdu"), &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(parsed.body, "Hello one to one");
        assert_eq!(
            parsed.participants,
            vec!["4075551234".to_string(), "5555550100".to_string()]
        );
        assert!(!parsed.is_sent);
        assert!(!parsed.is_group);
        assert_eq!(parsed.sender_number, "4075551234");
        assert_eq!(parsed.timestamp, 1609459200);
    }

    #[test]
    fn sent_one_to_one() {
        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&fixture("I_1609459200_sent.pdu"), &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(parsed.body, "Sent MMS");
        assert!(parsed.is_sent);
        assert!(!parsed.is_group);
        assert_eq!(parsed.sender_number, "5555550100");
    }

    #[test]
    fn group_pdu() {
        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&fixture("I_1609459200_group.pdu"), &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(parsed.body, "Group MMS body");
        assert!(parsed.is_group);
        assert_eq!(
            parsed.participants,
            vec![
                "5551112222".to_string(),
                "5552223333".to_string(),
                "5553334444".to_string(),
                "5555550100".to_string()
            ]
        );
        assert!(!parsed.is_sent);
        assert_eq!(parsed.sender_number, "5551112222");
    }

    #[test]
    fn jpeg_attachment() {
        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&fixture("I_1609459200_att.pdu"), &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(parsed.attachments[0].ext, ".jpg");
        assert!(parsed.attachments[0].data.len() >= 256);
    }
}
