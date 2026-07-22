//! Parse GO SMS Pro MMS PDU backup files (`I_<timestamp>_*.pdu`).
//!
//! Prefers WAP-209 / Content-Location / SMIL structured fields ([`crate::mms_enc`]),
//! then falls back to text-marker / magic-byte heuristics only when a field is empty.

use crate::emoji::decode_gosms_emojis;
use crate::mms_enc::{
    content_type_from_filename, decode_mms_best_effort, extension_for_content_type, NamedPart,
    StructuredMms,
};
use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use message_phone::sanitize_number;
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::bytes::Regex as BytesRegex;
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

static PDU_FILENAME_RE: OnceLock<Regex> = OnceLock::new();
static PLMN_RE: OnceLock<BytesRegex> = OnceLock::new();
static TEXT_CONTENT_RE: OnceLock<BytesRegex> = OnceLock::new();
static MMS_PART_JUNK_RE: OnceLock<Regex> = OnceLock::new();
static PRINTABLE_RUN_RE: OnceLock<BytesRegex> = OnceLock::new();
static TRAILING_GARBAGE_RE: OnceLock<Regex> = OnceLock::new();
static TEXT_PART_NAME_RE: OnceLock<Regex> = OnceLock::new();

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

#[derive(Debug, Default)]
struct SmilRefs {
    text_srcs: Vec<String>,
    media_srcs: Vec<String>,
}

fn timestamp_from_filename(name: &str) -> Option<i64> {
    let re = PDU_FILENAME_RE.get_or_init(|| Regex::new(r"^I_(?P<ts>\d+)_").expect("pdu name"));
    re.captures(name)
        .and_then(|c| c.name("ts"))
        .and_then(|m| m.as_str().parse().ok())
}

fn extract_plmn_numbers(data: &[u8]) -> Vec<String> {
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

/// Digits from an MMS address (`+1…/TYPE=PLMN` or bare digits).
fn digits_from_mms_address(addr: &str) -> Option<String> {
    let base = addr.split('/').next().unwrap_or(addr).trim();
    let trimmed = base.trim_start_matches('+');
    sanitize_number(trimmed).or_else(|| sanitize_number(base))
}

fn participants_from_structured(msg: &StructuredMms) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut numbers = Vec::new();
    for addr in msg.address_strings() {
        if let Some(digits) = digits_from_mms_address(&addr) {
            if seen.insert(digits.clone()) {
                numbers.push(digits);
            }
        }
    }
    numbers
}

fn is_text_part_name(name: &str) -> bool {
    let re = TEXT_PART_NAME_RE
        .get_or_init(|| Regex::new(r"(?i)^text(?:_\d+)?\.txt$").expect("text name"));
    re.is_match(name)
}

fn text_from_part_data(data: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(data)
        .replace('\0', "")
        .trim()
        .to_string();
    let text = truncate_mms_binary_tail(&text);
    if text.is_empty() || is_mms_part_junk(&text) {
        return None;
    }
    Some(decode_gosms_emojis(&text))
}

fn body_from_named_parts(named: &[NamedPart], smil: &SmilRefs) -> Option<String> {
    let by_name: HashMap<&str, &NamedPart> = named.iter().map(|p| (p.name.as_str(), p)).collect();
    for src in &smil.text_srcs {
        if let Some(part) = by_name.get(src.as_str()) {
            if let Some(text) = text_from_part_data(&part.data) {
                return Some(text);
            }
        }
    }
    let mut texts = Vec::new();
    let mut seen = HashSet::new();
    for part in named {
        if !is_text_part_name(&part.name)
            && !content_type_from_filename(&part.name).starts_with("text/")
        {
            continue;
        }
        if let Some(text) = text_from_part_data(&part.data) {
            if seen.insert(text.clone()) {
                texts.push(text);
            }
        }
    }
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

fn body_from_structured(msg: &StructuredMms) -> Option<String> {
    if msg.parts.is_empty() {
        return None;
    }
    let mut texts = Vec::new();
    let mut seen = HashSet::new();
    for part in &msg.parts {
        let ct = part.content_type.to_ascii_lowercase();
        let base = ct.split(';').next().unwrap_or(&ct).trim();
        if !(base.starts_with("text/plain") || base == "text/*" || base == "text/html") {
            continue;
        }
        if let Some(text) = text_from_part_data(&part.data) {
            if seen.insert(text.clone()) {
                texts.push(text);
            }
        }
    }
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

fn ext_from_filename(name: &str) -> Option<String> {
    let ct = content_type_from_filename(name);
    extension_for_content_type(&ct).map(|e| e.to_string())
}

fn attachment_ok(ext: &str, len: usize) -> bool {
    if len < 64 && matches!(ext, ".jpg" | ".png" | ".gif") {
        return false;
    }
    if ext == ".wav" && len < 10000 {
        return false;
    }
    true
}

fn attachments_from_named_parts(named: &[NamedPart], smil: &SmilRefs) -> Vec<ParsedAttachment> {
    let media_set: HashSet<&str> = smil.media_srcs.iter().map(String::as_str).collect();
    let mut out = Vec::new();
    for part in named {
        if is_text_part_name(&part.name) {
            continue;
        }
        let Some(ext) = ext_from_filename(&part.name) else {
            continue;
        };
        if ext == ".txt" {
            continue;
        }
        if !media_set.is_empty() && !media_set.contains(part.name.as_str()) {
            // SMIL listed media: only keep those srcs.
            continue;
        }
        if !attachment_ok(&ext, part.data.len()) {
            continue;
        }
        out.push(ParsedAttachment {
            ext,
            data: part.data.clone(),
            smil_name: Some(part.name.clone()),
        });
    }
    out
}

fn attachments_from_structured(msg: &StructuredMms) -> Vec<ParsedAttachment> {
    let mut out = Vec::new();
    for part in &msg.parts {
        let Some(ext) = extension_for_content_type(&part.content_type) else {
            continue;
        };
        if ext == ".txt" {
            continue;
        }
        if !attachment_ok(ext, part.data.len()) {
            continue;
        }
        out.push(ParsedAttachment {
            ext: ext.to_string(),
            data: part.data.clone(),
            smil_name: part.content_location.clone(),
        });
    }
    out
}

fn extract_smil_region<'a>(data: &'a [u8]) -> Option<&'a [u8]> {
    let lower = data.to_ascii_lowercase();
    let start = lower.windows(5).position(|w| w == b"<smil")?;
    let end_rel = lower[start..]
        .windows(7)
        .position(|w| w == b"</smil>")
        .map(|p| start + p + 7)?;
    Some(&data[start..end_rel])
}

fn parse_smil_refs(data: &[u8]) -> SmilRefs {
    let mut refs = SmilRefs::default();
    let Some(smil_bytes) = extract_smil_region(data) else {
        return refs;
    };
    let Ok(text) = std::str::from_utf8(smil_bytes) else {
        return refs;
    };
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_ascii_lowercase();
                let mut src = None;
                for attr in e.attributes().flatten() {
                    let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
                    if key == "src" {
                        src = Some(String::from_utf8_lossy(&attr.value).into_owned());
                    }
                }
                if let Some(s) = src {
                    if s.is_empty() {
                        continue;
                    }
                    match tag.as_str() {
                        "text" => refs.text_srcs.push(s),
                        "img" | "audio" | "video" => refs.media_srcs.push(s),
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    refs
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
    text_from_part_data(&data[start..end]).unwrap_or_default()
}

fn find_bytes(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    haystack[start..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| start + p)
}

/// Last-resort body when Content-Location / multipart text is missing.
fn extract_wap_text_body_fallback(data: &[u8]) -> String {
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

fn detect_attachment_blobs(data: &[u8]) -> Vec<(String, usize, usize)> {
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
        if !attachment_ok(ext, size) {
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
    sanitize_number(digits).is_some_and(|d| owners.contains(&d))
}

fn roles_from_structured(
    msg: &StructuredMms,
    owners: &HashSet<String>,
) -> (Option<String>, bool, bool) {
    let from_digits = msg
        .from
        .as_ref()
        .and_then(|a| digits_from_mms_address(a));
    let my_is_from = from_digits
        .as_ref()
        .is_some_and(|d| is_owner_digit(d, owners));
    let my_is_to = msg
        .to
        .iter()
        .chain(msg.cc.iter())
        .filter_map(|a| digits_from_mms_address(a))
        .any(|d| is_owner_digit(&d, owners));
    (from_digits, my_is_from, my_is_to)
}

/// Direction from decoded From/To/Cc when present; otherwise owner/participant rules
/// (no byte-prefix markers — sent fixtures often lack From/To headers entirely).
fn infer_pdu_direction(
    structured: &StructuredMms,
    unique_parts: &[String],
    owners: &HashSet<String>,
    primary_digits: &str,
) -> (bool, String) {
    if unique_parts.is_empty() {
        return (false, String::new());
    }

    let has_roles =
        structured.from.is_some() || !structured.to.is_empty() || !structured.cc.is_empty();

    if has_roles {
        let (from_digits, my_is_from, my_is_to) = roles_from_structured(structured, owners);
        if my_is_from {
            return (true, primary_digits.to_string());
        }
        if let Some(from) = from_digits {
            if !is_owner_digit(&from, owners) {
                return (false, from);
            }
            return (true, primary_digits.to_string());
        }
        if my_is_to {
            let sender = unique_parts
                .iter()
                .find(|p| !is_owner_digit(p, owners))
                .cloned()
                .unwrap_or_else(|| unique_parts[0].clone());
            return (false, sender);
        }
    }

    // Raw PLMN lists without From/To headers (e.g. sent one-to-one dumps).
    if unique_parts.len() >= 3 {
        let sender = unique_parts
            .iter()
            .find(|p| !is_owner_digit(p, owners))
            .cloned()
            .unwrap_or_else(|| unique_parts[0].clone());
        return (false, sender);
    }

    if unique_parts.iter().any(|p| is_owner_digit(p, owners)) {
        return (true, primary_digits.to_string());
    }

    (false, unique_parts[0].clone())
}

fn resolve_timestamp(filename_ts: i64, structured: &StructuredMms) -> i64 {
    match structured.date_unix {
        Some(d) if d > 0 && d <= i64::MAX as u64 => d as i64,
        _ => filename_ts,
    }
}

/// Parse one PDU file. Returns `None` for unparseable / bad filenames.
pub fn parse_pdu_file(path: &Path, owners: &HashSet<String>, primary_digits: &str) -> Result<Option<ParsedPdu>> {
    let data = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    if data.len() < 10 {
        return Ok(None);
    }
    let Some(filename_ts) = timestamp_from_filename(
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(""),
    ) else {
        return Ok(None);
    };

    let structured = decode_mms_best_effort(&data);
    let smil = parse_smil_refs(&data);
    let timestamp = resolve_timestamp(filename_ts, &structured);

    let participants_raw = {
        let mut parts = participants_from_structured(&structured);
        if parts.is_empty() {
            extract_plmn_numbers(&data)
        } else {
            let mut seen: HashSet<String> = parts.iter().cloned().collect();
            for n in extract_plmn_numbers(&data) {
                if seen.insert(n.clone()) {
                    parts.push(n);
                }
            }
            parts
        }
    };

    let body = body_from_named_parts(&structured.named_parts, &smil)
        .or_else(|| body_from_structured(&structured))
        .unwrap_or_else(|| extract_wap_text_body_fallback(&data));

    let mut attachments = attachments_from_named_parts(&structured.named_parts, &smil);
    if attachments.is_empty() {
        attachments = attachments_from_structured(&structured);
    }
    if attachments.is_empty() {
        let blobs = detect_attachment_blobs(&data);
        for (i, (ext, start, end)) in blobs.into_iter().enumerate() {
            let smil_name = smil.media_srcs.get(i).cloned();
            attachments.push(ParsedAttachment {
                ext,
                data: data[start..end].to_vec(),
                smil_name,
            });
        }
    }

    let normalized_parts: Vec<String> = participants_raw
        .iter()
        .filter_map(|p| sanitize_number(p))
        .collect();
    let unique_parts = unique_participants(&normalized_parts);
    let is_group = unique_parts.len() >= 3;
    let (is_sent, sender_number) =
        infer_pdu_direction(&structured, &unique_parts, owners, primary_digits);

    Ok(Some(ParsedPdu {
        path: path.to_path_buf(),
        timestamp,
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

    #[test]
    fn body_from_content_location_without_marker_regex() {
        let data = std::fs::read(fixture("I_1609459200_recv.pdu")).unwrap();
        let structured = decode_mms_best_effort(&data);
        let smil = SmilRefs::default();
        let body = body_from_named_parts(&structured.named_parts, &smil).expect("named body");
        assert_eq!(body, "Hello one to one");
    }

    #[test]
    fn smil_binds_text_and_image_parts() {
        let mut data = Vec::new();
        data.extend_from_slice(b"<smil><body><text src=\"text.txt\"/><img src=\"IMG_1.jpg\"/></body></smil>");
        data.extend_from_slice(&[0x8e]);
        data.extend_from_slice(b"text.txt\0Hello from SMIL");
        data.extend_from_slice(&[0x8e]);
        data.extend_from_slice(b"IMG_1.jpg\0");
        // Minimal JPEG large enough to pass size guard
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0];
        jpeg.extend(std::iter::repeat_n(0x00, 80));
        data.extend_from_slice(&jpeg);

        let structured = decode_mms_best_effort(&data);
        let smil = parse_smil_refs(&data);
        assert_eq!(smil.text_srcs, vec!["text.txt".to_string()]);
        assert_eq!(smil.media_srcs, vec!["IMG_1.jpg".to_string()]);
        let body = body_from_named_parts(&structured.named_parts, &smil).unwrap();
        assert_eq!(body, "Hello from SMIL");
        let atts = attachments_from_named_parts(&structured.named_parts, &smil);
        assert_eq!(atts.len(), 1);
        assert_eq!(atts[0].ext, ".jpg");
        assert_eq!(atts[0].smil_name.as_deref(), Some("IMG_1.jpg"));
    }

    #[test]
    fn mms_date_overrides_filename_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        // Filename says 1609459200; Date header says 1700000000
        let path = dir.path().join("I_1609459200_dated.pdu");
        let mut bytes = vec![0x85, 0x04, 0x65, 0x53, 0xf1, 0x00]; // 1700000000
        bytes.extend_from_slice(&[0x89, 0x1a, 0x80, 0x18, 0xea]);
        bytes.extend_from_slice(b"+4075551234/TYPE=PLMN");
        bytes.extend_from_slice(&[0x97, 0x18, 0xea]);
        bytes.extend_from_slice(b"+15555550100/TYPE=PLMN");
        bytes.extend_from_slice(&[0x8e]);
        bytes.extend_from_slice(b"text.txt\0Dated body");
        // Pad so To value-length overshoot has a following header byte
        bytes.push(0x8c);
        std::fs::write(&path, &bytes).unwrap();

        let (owners, primary) = test_owners();
        let parsed = parse_pdu_file(&path, &owners, &primary)
            .unwrap()
            .expect("parsed");
        assert_eq!(parsed.timestamp, 1700000000);
        assert_eq!(parsed.body, "Dated body");
    }
}
