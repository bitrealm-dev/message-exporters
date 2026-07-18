//! Phone normalization helpers (US-centric).

/// Strip non-digits and a leading US country code `1`.
/// Returns `None` when no usable digits remain.
pub fn sanitize_number(num: &str) -> Option<String> {
    if num.is_empty() {
        return None;
    }
    let mut digits: String = num.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 11 && digits.starts_with('1') {
        digits = digits[1..].to_string();
    }
    if digits.is_empty() {
        None
    } else {
        Some(digits)
    }
}

/// Format already-sanitized digits as E.164 (`+1…` for 10-digit US).
pub fn to_e164(digits: &str) -> String {
    if digits.len() == 10 {
        format!("+1{digits}")
    } else if digits.starts_with('+') {
        digits.to_string()
    } else {
        format!("+{digits}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_plus_one() {
        assert_eq!(sanitize_number("+15555550100").as_deref(), Some("5555550100"));
        assert_eq!(sanitize_number("(555) 555-0101").as_deref(), Some("5555550101"));
        assert_eq!(sanitize_number(""), None);
        assert_eq!(sanitize_number("abc"), None);
    }

    #[test]
    fn e164_us() {
        assert_eq!(to_e164("5555550100"), "+15555550100");
    }
}
