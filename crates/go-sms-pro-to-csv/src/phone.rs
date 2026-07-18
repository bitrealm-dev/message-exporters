//! GO SMS Pro–specific phone helpers (shared sanitize lives in `message-phone`).

use message_phone::sanitize_number;
use regex::Regex;
use std::sync::OnceLock;

static GV_RE: OnceLock<Regex> = OnceLock::new();

/// Extract caller digits from a Google Voice voicemail SMS body.
pub fn parse_google_voice_voicemail_caller(body: &str) -> Option<String> {
    let re = GV_RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:\(1/\d+\)\s*)?you've got a new voicemail from \((\d{3})\)\s*([\d-]+)",
        )
        .expect("gv regex")
    });
    let caps = re.captures(body)?;
    let digits = sanitize_number(&format!("{}{}", &caps[1], &caps[2]))?;
    if digits.len() < 10 {
        None
    } else {
        Some(digits)
    }
}
