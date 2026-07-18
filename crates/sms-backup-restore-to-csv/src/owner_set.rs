//! All configured vault-owner phone numbers (normalized digits).

use std::collections::HashSet;

use anyhow::{bail, Context, Result};

use crate::phone::{sanitize_number, to_e164};

#[derive(Debug, Clone)]
pub struct OwnerPhoneSet {
    pub all_digits: HashSet<String>,
    pub primary_digits: String,
    pub primary_e164: String,
}

impl OwnerPhoneSet {
    pub fn new(phones: &[String]) -> Result<Self> {
        if phones.is_empty() {
            bail!("owner.phones must not be empty");
        }
        let mut all_digits = HashSet::new();
        for phone in phones {
            let d = sanitize_number(phone)
                .with_context(|| format!("owner phone has no usable digits: {phone}"))?;
            all_digits.insert(d);
        }
        let primary_digits = sanitize_number(&phones[0])
            .context("owner phone has no usable digits")?;
        let primary_e164 = to_e164(&primary_digits);
        Ok(Self {
            all_digits,
            primary_digits,
            primary_e164,
        })
    }

    pub fn is_owner(&self, digits: &str) -> bool {
        sanitize_number(digits)
            .is_some_and(|d| self.all_digits.contains(&d))
    }
}
