//! Incorrect EML export name → correct contact display name.

use crate::name::{collapse_inner_whitespace, normalize_name_key};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Incorrect EML export name → correct contact display name.
#[derive(Debug, Default, Clone)]
pub struct NameMapping {
    /// Normalized incorrect name → correct display name (as written in CSV).
    incorrect_to_correct: HashMap<String, String>,
}

impl NameMapping {
    pub fn empty() -> Self {
        Self {
            incorrect_to_correct: HashMap::new(),
        }
    }

    /// Load `correct_name,incorrect_name` CSV.
    pub fn load(path: &Path) -> Result<Self> {
        let file =
            File::open(path).with_context(|| format!("open name mapping {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let header = lines.next().transpose()?.unwrap_or_default();
        let header_l = header.to_ascii_lowercase();
        if !header_l.contains("correct_name") || !header_l.contains("incorrect_name") {
            anyhow::bail!(
                "name mapping CSV {} missing expected header correct_name,incorrect_name",
                path.display()
            );
        }

        let mut mapping = Self::empty();
        for (idx, line) in lines.enumerate() {
            let line = line.with_context(|| format!("read name mapping line {}", idx + 2))?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts = crate::book::split_csv_line(line);
            if parts.len() < 2 {
                continue;
            }
            let correct = collapse_inner_whitespace(parts[0].trim());
            let incorrect = collapse_inner_whitespace(parts[1].trim());
            // Empty correct_name means “no usable contact” — do not map.
            if correct.is_empty() || incorrect.is_empty() {
                continue;
            }
            let key = normalize_name_key(&incorrect);
            if key.is_empty() {
                continue;
            }
            mapping.incorrect_to_correct.entry(key).or_insert(correct);
        }
        Ok(mapping)
    }

    pub fn load_optional(path: Option<&Path>) -> Result<(Self, Option<std::path::PathBuf>)> {
        match path {
            Some(path) => Ok((Self::load(path)?, Some(path.to_path_buf()))),
            None => Ok((Self::empty(), None)),
        }
    }

    /// If `eml_name` is an incorrect export name, return the correct display name.
    pub fn correct_name(&self, eml_name: &str) -> Option<&str> {
        let key = normalize_name_key(eml_name);
        if key.is_empty() {
            return None;
        }
        self.incorrect_to_correct.get(&key).map(String::as_str)
    }
}
