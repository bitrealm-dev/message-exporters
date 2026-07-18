//! Phone normalization helpers (US-centric).

/// Minimum digit length after stripping formatting.
///
/// Allows 5–6 digit short codes (carrier/bank SMS). Rejects archive junk like
/// `"4"` or `"06"`.
const MIN_PHONE_DIGITS: usize = 5;

pub(crate) fn sanitize_number(num: &str) -> String {
    if num.is_empty() {
        return "Unknown".to_string();
    }
    let mut digits: String = num.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 11 && digits.starts_with('1') {
        digits = digits[1..].to_string();
    }
    if digits.len() < MIN_PHONE_DIGITS {
        "Unknown".to_string()
    } else {
        digits
    }
}

pub(crate) fn to_e164(digits: &str) -> String {
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

pub(crate) fn owner_digits(owner_phone: &str) -> String {
    sanitize_number(owner_phone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_plus_one() {
        assert_eq!(sanitize_number("+15555550100"), "5555550100");
    }

    #[test]
    fn sanitize_rejects_short_digit_runs() {
        assert_eq!(sanitize_number("4"), "Unknown");
        assert_eq!(sanitize_number("06"), "Unknown");
        assert_eq!(sanitize_number("+1"), "Unknown");
    }

    #[test]
    fn sanitize_keeps_short_codes() {
        assert_eq!(sanitize_number("73737"), "73737");
        assert_eq!(sanitize_number("239663"), "239663");
        assert_eq!(to_e164("73737"), "+73737");
    }
}
