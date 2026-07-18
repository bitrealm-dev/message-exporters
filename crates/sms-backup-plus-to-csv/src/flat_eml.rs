//! Parse flat EMLs: one text message per `.eml` file (not a multi-message archive).

use crate::assets::extract_attachments;
use crate::phone::{sanitize_number, to_e164};
use crate::types::ParsedMessage;
use anyhow::Result;
use mailparse::{MailHeaderMap, ParsedMail};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

static SUBJECT_RE: OnceLock<Regex> = OnceLock::new();
static ADDRESS_SPLIT_RE: OnceLock<Regex> = OnceLock::new();
static ARCHIVE_SUBJECT_PREFIX_RE: OnceLock<Regex> = OnceLock::new();

const SENT_TYPES: &[&str] = &["2", "128", "4", "135", "6", "5"];
const RECEIVED_TYPES: &[&str] = &["1", "132", "130"];

fn subject_re() -> &'static Regex {
    SUBJECT_RE.get_or_init(|| Regex::new(r"(?i)^SMS with (.+)$").expect("subject"))
}

fn address_split_re() -> &'static Regex {
    ADDRESS_SPLIT_RE.get_or_init(|| Regex::new(r"[~;,|]+").expect("split"))
}

fn archive_subject_prefix_re() -> &'static Regex {
    ARCHIVE_SUBJECT_PREFIX_RE
        .get_or_init(|| Regex::new(r"(?i)^SMS archive ").expect("archive subject"))
}

fn header(mail: &ParsedMail<'_>, name: &str) -> String {
    mail.headers
        .get_first_value(name)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn smssync_participant_numbers(raw_address: &str) -> Vec<String> {
    if raw_address.trim().is_empty() {
        return Vec::new();
    }
    let mut numbers = Vec::new();
    let mut seen = HashSet::new();
    for part in address_split_re().split(raw_address) {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        let num = sanitize_number(token);
        if num == "Unknown" || !seen.insert(num.clone()) {
            continue;
        }
        numbers.push(num);
    }
    numbers
}

fn contact_name_from_subject(subject: &str) -> Option<String> {
    let caps = subject_re().captures(subject.trim())?;
    let name = caps[1].trim();
    if name.starts_with('+') || name.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(name.to_string())
}

fn timestamp_seconds(mail: &ParsedMail<'_>) -> Option<f64> {
    let raw = header(mail, "X-smssync-date");
    if !raw.is_empty() && raw.chars().all(|c| c.is_ascii_digit()) {
        let value: i64 = raw.parse().ok()?;
        // Android uses epoch ms (~1e12 today). Seconds stay ~1e9 until year 5138.
        // Threshold 1e11 catches pre-2001 ms timestamps that the old 1e12 cutoff missed.
        return Some(if value >= 100_000_000_000 {
            value as f64 / 1000.0
        } else {
            value as f64
        });
    }
    let date_hdr = header(mail, "Date");
    if date_hdr.is_empty() {
        return None;
    }
    // mailparse doesn't parse dates for us; try chrono RFC2822
    chrono::DateTime::parse_from_rfc2822(&date_hdr)
        .ok()
        .map(|d| d.timestamp() as f64)
}

fn is_sent(mail: &ParsedMail<'_>, owner_emails: &[String]) -> bool {
    let typ = header(mail, "X-smssync-type");
    if SENT_TYPES.contains(&typ.as_str()) {
        return true;
    }
    if RECEIVED_TYPES.contains(&typ.as_str()) {
        return false;
    }
    let from = header(mail, "From").to_ascii_lowercase();
    owner_emails.iter().any(|e| {
        let e = e.trim().to_ascii_lowercase();
        !e.is_empty() && from.contains(&e)
    })
}

fn extract_body(mail: &ParsedMail<'_>) -> String {
    fn walk(m: &ParsedMail<'_>) -> Option<String> {
        let ctype = m.ctype.mimetype.to_ascii_lowercase();
        if ctype == "text/plain"
            && let Ok(body) = m.get_body()
        {
            return Some(body.replace("\r\n", "\n").replace('\r', "\n"));
        }
        for part in &m.subparts {
            if let Some(b) = walk(part) {
                return Some(b);
            }
        }
        None
    }
    walk(mail).unwrap_or_default()
}

fn is_single_sms_eml(mail: &ParsedMail<'_>) -> bool {
    if !header(mail, "X-smssync-type").is_empty() {
        return true;
    }
    let subject = header(mail, "Subject");
    let headers_blob = format!("{} {}", header(mail, "From"), header(mail, "To"));
    subject_re().is_match(&subject) && headers_blob.contains("@sms-backup-plus.local")
}

/// True when this looks like a flat single-message SMS Backup+ EML.
pub(crate) fn is_flat_sms_eml(mail: &ParsedMail<'_>) -> bool {
    is_single_sms_eml(mail)
}

fn group_chat_id(others: &[String]) -> (String, String) {
    let mut sorted = others.to_vec();
    sorted.sort();
    sorted.dedup();
    let title = if sorted.is_empty() {
        "Group".to_string()
    } else if sorted.len() <= 4 {
        format!(
            "Group: {}",
            sorted
                .iter()
                .map(|d| to_e164(d))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!(
            "Group: {}, and {} others",
            sorted[..4]
                .iter()
                .map(|d| to_e164(d))
                .collect::<Vec<_>>()
                .join(", "),
            sorted.len() - 4
        )
    };
    let key = format!("group-{}", sorted.join("_"));
    let key = if key.len() > 180 {
        let digest = hex::encode(Sha256::digest(key.as_bytes()));
        format!("group-{}", &digest[..16])
    } else {
        key
    };
    (key, title)
}

/// Parse an already-loaded flat SMS Backup+ EML (avoids a second disk read).
pub(crate) fn parse_flat_eml_mail(
    path: &Path,
    mail: &ParsedMail<'_>,
    owner_digits: &HashSet<String>,
    owner_emails: &[String],
) -> Result<Option<ParsedMessage>> {
    if !is_single_sms_eml(mail) {
        return Ok(None);
    }

    let Some(ts) = timestamp_seconds(mail) else {
        return Ok(None);
    };

    let subject = header(mail, "Subject");
    let subject_name = contact_name_from_subject(&subject);
    let mut addr_raw = header(mail, "X-smssync-address");
    if addr_raw.is_empty()
        && let Some(ref name) = subject_name
    {
        addr_raw = name.clone();
    }
    let participant_numbers = smssync_participant_numbers(&addr_raw);
    let addr = participant_numbers
        .first()
        .cloned()
        .unwrap_or_else(|| sanitize_number(&addr_raw));
    if addr == "Unknown" && addr_raw.is_empty() {
        return Ok(None);
    }

    let name_hint = subject_name.clone();
    let sent = is_sent(mail, owner_emails);
    let body = extract_body(mail);

    let file_key = hex::encode(Sha256::digest(path.to_string_lossy().as_bytes()));
    let file_key = &file_key[..12.min(file_key.len())];
    let attachments = extract_attachments(mail, ts * 1000.0, Some(file_key));

    let non_owner: Vec<String> = participant_numbers
        .iter()
        .filter(|n| !owner_digits.contains(*n))
        .cloned()
        .collect();

    if non_owner.len() >= 2 {
        let (is_from_me, sender_digits) = if sent {
            (true, None)
        } else {
            // Prefer From header digits among participants
            let from_hdr = header(mail, "From");
            let from_nums = smssync_participant_numbers(&from_hdr);
            let sender = from_nums
                .into_iter()
                .find(|n| !owner_digits.contains(n) && non_owner.contains(n))
                .or_else(|| non_owner.first().cloned());
            (false, sender)
        };
        let (chat_key, title) = group_chat_id(&non_owner);
        let participant_digits: Vec<_> = non_owner.into_iter().map(|d| (d, None)).collect();
        let smssync_id = {
            let raw = header(mail, "X-smssync-id");
            if raw.is_empty() { None } else { Some(raw) }
        };
        let android_type = header(mail, "X-smssync-type");
        return Ok(Some(ParsedMessage {
            chat_key,
            conversation_type: "group".into(),
            group_title: Some(title),
            participant_digits,
            timestamp_secs: ts,
            is_from_me,
            sender_digits,
            text: body,
            attachments,
            name_hint,
            smssync_id,
            source_kind: "flat".into(),
            android_type,
            eml_path: String::new(),
        }));
    }

    let conv_number = if addr != "Unknown" {
        addr.clone()
    } else {
        sanitize_number(&addr_raw)
    };
    // Keep Unknown when we have a display name so contacts reverse-lookup can fill it.
    if conv_number == "Unknown"
        && name_hint
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
    {
        return Ok(None);
    }

    let smssync_id = {
        let raw = header(mail, "X-smssync-id");
        if raw.is_empty() { None } else { Some(raw) }
    };
    let android_type = header(mail, "X-smssync-type");
    Ok(Some(ParsedMessage {
        chat_key: conv_number.clone(),
        conversation_type: "individual".into(),
        group_title: None,
        participant_digits: if conv_number == "Unknown" {
            vec![]
        } else {
            vec![(conv_number.clone(), name_hint.clone())]
        },
        timestamp_secs: ts,
        is_from_me: sent,
        sender_digits: if sent || conv_number == "Unknown" {
            None
        } else {
            Some(conv_number)
        },
        text: body,
        attachments,
        name_hint,
        smssync_id,
        source_kind: "flat".into(),
        android_type,
        eml_path: String::new(),
    }))
}

/// Classify whether this EML looks like a consolidated archive thread.
pub(crate) fn is_archive_eml(mail: &ParsedMail<'_>) -> bool {
    let subject = header(mail, "Subject");
    archive_subject_prefix_re().is_match(subject.trim())
        && header(mail, "X-smssync-type").is_empty()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_received() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("msg.eml");
        std::fs::write(
            &path,
            b"From: alice@unknown.email\r\n\
To: me@example.com\r\n\
Subject: SMS with Alice\r\n\
X-smssync-type: 1\r\n\
X-smssync-address: 4075551234\r\n\
X-smssync-date: 1609459200000\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Hello from Alice\r\n",
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let owners = HashSet::from(["5555550100".to_string()]);
        let msg = parse_flat_eml_mail(&path, &mail, &owners, &[])
            .unwrap()
            .unwrap();
        assert!(!msg.is_from_me);
        assert_eq!(msg.text.trim(), "Hello from Alice");
        assert_eq!(msg.chat_key, "4075551234");
        assert!((msg.timestamp_secs - 1_609_459_200.0).abs() < 0.001);
    }

    #[test]
    fn early_2000s_ms_dates_are_not_treated_as_seconds() {
        // 2001-01-01T00:00:00Z as epoch ms is < 1e12; the old >1e12 cutoff
        // would have treated this as seconds (~year 32995).
        let ms = 978_307_200_000_i64;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("msg.eml");
        std::fs::write(
            &path,
            format!(
                "From: alice@unknown.email\r\n\
To: me@example.com\r\n\
Subject: SMS with Alice\r\n\
X-smssync-type: 1\r\n\
X-smssync-address: 4075551234\r\n\
X-smssync-date: {ms}\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
old message\r\n"
            ),
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let mail = mailparse::parse_mail(&bytes).unwrap();
        let owners = HashSet::from(["5555550100".to_string()]);
        let msg = parse_flat_eml_mail(&path, &mail, &owners, &[])
            .unwrap()
            .unwrap();
        assert!((msg.timestamp_secs - 978_307_200.0).abs() < 0.001);
    }
}
