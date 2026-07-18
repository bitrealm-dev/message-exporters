//! Phone normalization helpers (US-centric, matching message-vault GoSMS convert).

use regex::Regex;
use std::sync::OnceLock;

/// Strip non-digits and leading US country code `1`. Returns 10-digit US form when possible.
pub fn sanitize_number(num: &str) -> String {
    if num.is_empty() {
        return "Unknown".to_string();
    }
    let mut digits: String = num.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 11 && digits.starts_with('1') {
        digits = digits[1..].to_string();
    }
    if digits.is_empty() {
        "Unknown".to_string()
    } else {
        digits
    }
}

/// Format sanitized digits as E.164 (`+1…` for 10-digit US).
pub fn to_e164(digits: &str) -> String {
    let d = sanitize_number(digits);
    if d == "Unknown" {
        return d;
    }
    if d.len() == 10 {
        format!("+1{d}")
    } else if d.starts_with('+') {
        d
    } else {
        format!("+{d}")
    }
}

/// Owner digits without country code for PDU comparisons.
pub fn owner_digits(owner_phone: &str) -> String {
    sanitize_number(owner_phone)
}

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
    let digits = sanitize_number(&format!("{}{}", &caps[1], &caps[2]));
    if digits.len() >= 10 {
        Some(digits)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_plus_one() {
        assert_eq!(sanitize_number("+15555550100"), "5555550100");
        assert_eq!(sanitize_number("(407) 555-1234"), "4075551234");
    }

    #[test]
    fn e164_us() {
        assert_eq!(to_e164("5555550100"), "+15555550100");
    }
}
