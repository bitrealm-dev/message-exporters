//! Minimal VCF 3.0 parser (phones + names) for contact books.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct VcfCard {
    pub fn_raw: String,
    pub n_family: String,
    pub n_given: String,
    pub phones: Vec<String>,
}

/// Parse a VCF file into cards (unfolded lines).
pub fn parse_vcf(path: &Path) -> Result<Vec<VcfCard>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read VCF {}", path.display()))?;
    let lines = unfold_lines(&text);
    let mut cards = Vec::new();
    let mut current: Option<VcfCard> = None;

    for line in lines {
        if line.eq_ignore_ascii_case("BEGIN:VCARD") {
            current = Some(VcfCard::default());
            continue;
        }
        if line.eq_ignore_ascii_case("END:VCARD") {
            if let Some(card) = current.take() {
                cards.push(card);
            }
            continue;
        }
        let Some(card) = current.as_mut() else {
            continue;
        };
        apply_line(card, &line);
    }

    Ok(cards)
}

fn unfold_lines(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = out.last_mut() {
                last.push_str(&line[1..]);
            }
            continue;
        }
        out.push(line.to_string());
    }
    out
}

fn apply_line(card: &mut VcfCard, line: &str) {
    let Some((name, value)) = line.split_once(':') else {
        return;
    };
    let prop = name.split(';').next().unwrap_or(name);
    let prop_upper = prop.to_ascii_uppercase();
    let base = prop_upper
        .rsplit_once('.')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or(prop_upper);

    match base.as_str() {
        "FN" => card.fn_raw = unescape(value),
        "N" => {
            let parts: Vec<&str> = value.split(';').collect();
            card.n_family = unescape(parts.first().copied().unwrap_or(""));
            card.n_given = unescape(parts.get(1).copied().unwrap_or(""));
        }
        "TEL" => {
            let phone = value.trim();
            if !phone.is_empty() && !card.phones.iter().any(|p| p == phone) {
                card.phones.push(phone.to_string());
            }
        }
        _ => {}
    }
}

fn unescape(s: &str) -> String {
    s.replace("\\n", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
        .trim()
        .to_string()
}

/// Strip `[Tag]` markers from a VCF name field.
pub fn strip_tags(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '[' {
            for c in chars.by_ref() {
                if c == ']' {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
