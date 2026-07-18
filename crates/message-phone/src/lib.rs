//! Shared US-centric phone normalization for message exporters.

use std::collections::HashSet;

use anyhow::{bail, Context, Result};

/// Minimum digit length after stripping formatting.
///
/// Allows 5–6 digit short codes (carrier/bank SMS). Rejects junk like `"4"`.
const MIN_PHONE_DIGITS: usize = 5;

/// Strip non-digits and a leading US country code `1`.
/// Returns `None` when fewer than [`MIN_PHONE_DIGITS`] remain.
pub fn sanitize_number(num: &str) -> Option<String> {
    if num.is_empty() {
        return None;
    }
    let mut digits: String = num.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 11 && digits.starts_with('1') {
        digits = digits[1..].to_string();
    }
    if digits.len() < MIN_PHONE_DIGITS {
        None
    } else {
        Some(digits)
    }
}

/// Format already-sanitized digits as E.164 (`+1…` for 10-digit US).
///
/// Pass the output of [`sanitize_number`], not a raw user string.
pub fn to_e164(digits: &str) -> String {
    if digits.len() == 10 {
        format!("+1{digits}")
    } else if digits.starts_with('+') {
        digits.to_string()
    } else {
        format!("+{digits}")
    }
}

/// All configured owner phone numbers (normalized digits).
#[derive(Debug, Clone)]
pub struct OwnerPhoneSet {
    pub all_digits: HashSet<String>,
    pub primary_digits: String,
}

impl OwnerPhoneSet {
    pub fn new(phones: &[String]) -> Result<Self> {
        if phones.is_empty() {
            bail!("owner phone required: pass --owner-phone (or set phones in config)");
        }
        let mut all_digits = HashSet::new();
        for phone in phones {
            let d = sanitize_number(phone)
                .with_context(|| format!("owner phone has no usable digits: {phone}"))?;
            all_digits.insert(d);
        }
        let primary_digits = sanitize_number(&phones[0])
            .context("owner phone has no usable digits")?;
        Ok(Self {
            all_digits,
            primary_digits,
        })
    }

    pub fn is_owner(&self, digits: &str) -> bool {
        sanitize_number(digits).is_some_and(|d| self.all_digits.contains(&d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_plus_one() {
        assert_eq!(
            sanitize_number("+15555550100").as_deref(),
            Some("5555550100")
        );
        assert_eq!(
            sanitize_number("(555) 555-0101").as_deref(),
            Some("5555550101")
        );
        assert_eq!(sanitize_number(""), None);
        assert_eq!(sanitize_number("4"), None);
        assert_eq!(sanitize_number("06"), None);
    }

    #[test]
    fn sanitize_keeps_short_codes() {
        assert_eq!(sanitize_number("73737").as_deref(), Some("73737"));
        assert_eq!(to_e164("73737"), "+73737");
    }

    #[test]
    fn e164_us() {
        assert_eq!(to_e164("5555550100"), "+15555550100");
    }

    #[test]
    fn owner_set_rejects_empty() {
        assert!(OwnerPhoneSet::new(&[]).is_err());
        assert!(OwnerPhoneSet::new(&["not-a-phone".into()]).is_err());
    }
}
