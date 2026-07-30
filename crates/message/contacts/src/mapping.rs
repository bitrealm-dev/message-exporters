//! Incorrect EML export name → phone number.

use crate::name::{collapse_inner_whitespace, normalize_name_key};
use anyhow::{Context, Result};
use phone::sanitize_number;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Incorrect EML export name → sanitized phone digits.
#[derive(Debug, Default, Clone)]
pub struct NameMapping {
    /// Normalized incorrect name → sanitized phone digits.
    incorrect_to_phone: HashMap<String, String>,
}

impl NameMapping {
    pub fn empty() -> Self {
        Self {
            incorrect_to_phone: HashMap::new(),
        }
    }

    /// Load `Phone,Incorrect Name` CSV (column order flexible; header required).
    pub fn load(path: &Path) -> Result<Self> {
        let file =
            File::open(path).with_context(|| format!("open name mapping {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let header = lines.next().transpose()?.unwrap_or_default();
        let header_parts = crate::book::split_csv_line(&header);
        let header_l: Vec<String> = header_parts
            .iter()
            .map(|h| h.trim().to_ascii_lowercase().replace('_', " "))
            .collect();

        let phone_idx = header_l.iter().position(|h| h == "phone");
        let incorrect_idx = header_l
            .iter()
            .position(|h| h == "incorrect name" || h == "incorrectname" || h == "incorrect");
        let (Some(phone_idx), Some(incorrect_idx)) = (phone_idx, incorrect_idx) else {
            anyhow::bail!(
                "name mapping CSV {} missing expected header Phone,Incorrect Name",
                path.display()
            );
        };

        let mut mapping = Self::empty();
        for (idx, line) in lines.enumerate() {
            let line = line.with_context(|| format!("read name mapping line {}", idx + 2))?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts = crate::book::split_csv_line(line);
            let phone_raw = parts.get(phone_idx).map(|s| s.trim()).unwrap_or("");
            let incorrect = parts
                .get(incorrect_idx)
                .map(|s| collapse_inner_whitespace(s.trim()))
                .unwrap_or_default();
            if phone_raw.is_empty() || incorrect.is_empty() {
                continue;
            }
            let Some(digits) = sanitize_number(phone_raw) else {
                continue;
            };
            let key = normalize_name_key(&incorrect);
            if key.is_empty() {
                continue;
            }
            mapping.incorrect_to_phone.entry(key).or_insert(digits);
        }
        Ok(mapping)
    }

    pub fn load_optional(path: Option<&Path>) -> Result<(Self, Option<std::path::PathBuf>)> {
        match path {
            Some(path) => Ok((Self::load(path)?, Some(path.to_path_buf()))),
            None => Ok((Self::empty(), None)),
        }
    }

    /// If `eml_name` is an incorrect export name, return sanitized phone digits.
    pub fn phone_for_incorrect_name(&self, eml_name: &str) -> Option<&str> {
        let key = normalize_name_key(eml_name);
        if key.is_empty() {
            return None;
        }
        self.incorrect_to_phone.get(&key).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.incorrect_to_phone.len()
    }

    pub fn is_empty(&self) -> bool {
        self.incorrect_to_phone.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_phone_incorrect_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("map.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            "Phone,Incorrect Name\n\
+15555550144,Jordan Alias (SKIP)\n\
15555550155,Casey Typo\n"
        )
        .unwrap();
        let mapping = NameMapping::load(&path).unwrap();
        assert_eq!(
            mapping.phone_for_incorrect_name("Jordan Alias (SKIP)"),
            Some("5555550144")
        );
        assert_eq!(
            mapping.phone_for_incorrect_name("casey typo"),
            Some("5555550155")
        );
    }

    #[test]
    fn accepts_reversed_columns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("map.csv");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            "Incorrect Name,Phone\n\
Jordan Alias (SKIP),+15555550144\n"
        )
        .unwrap();
        let mapping = NameMapping::load(&path).unwrap();
        assert_eq!(
            mapping.phone_for_incorrect_name("Jordan Alias (SKIP)"),
            Some("5555550144")
        );
    }
}
