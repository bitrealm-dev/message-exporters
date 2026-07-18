//! All configured owner phone numbers (normalized digits).

use std::collections::HashSet;

use anyhow::{bail, Result};

use crate::phone::{owner_digits, sanitize_number, to_e164};

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
            let d = owner_digits(phone);
            if d.is_empty() || d == "Unknown" {
                bail!("owner phone has no usable digits: {phone}");
            }
            all_digits.insert(d);
        }
        let primary_digits = owner_digits(&phones[0]);
        let primary_e164 = to_e164(&primary_digits);
        Ok(Self {
            all_digits,
            primary_digits,
            primary_e164,
        })
    }

    pub fn is_owner(&self, digits: &str) -> bool {
        self.all_digits.contains(&sanitize_number(digits))
    }
}
