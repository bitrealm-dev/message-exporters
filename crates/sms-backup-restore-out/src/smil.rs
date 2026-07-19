//! Parse SMIL layout metadata from Android MMS exports.

use base64::Engine;
use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

use crate::xml::MmsPart;

static TEXT_SRC_RE: OnceLock<Regex> = OnceLock::new();
static IMG_SRC_RE: OnceLock<Regex> = OnceLock::new();

pub fn smil_xml_from_parts(parts: &[MmsPart]) -> String {
    for part in parts {
        if part.ct.to_ascii_lowercase() != "application/smil" {
            continue;
        }
        let raw = part.text.trim();
        if !raw.is_empty() {
            return html_escape::decode_html_entities(raw).into_owned();
        }
        let data_b64 = part.data.trim();
        if data_b64.is_empty() || data_b64.eq_ignore_ascii_case("null") {
            continue;
        }
        if let Ok(payload) = base64::engine::general_purpose::STANDARD.decode(data_b64) {
            if !payload.is_empty() {
                return String::from_utf8_lossy(&payload).into_owned();
            }
        }
    }
    String::new()
}

pub fn ordered_smil_refs(smil_xml: &str) -> (Vec<String>, Vec<String>) {
    if smil_xml.trim().is_empty() {
        return (Vec::new(), Vec::new());
    }
    let text_re = TEXT_SRC_RE.get_or_init(|| {
        Regex::new(r#"(?i)<text[^>]+src=["']([^"']+)["']"#).expect("text src")
    });
    let img_re = IMG_SRC_RE
        .get_or_init(|| Regex::new(r#"(?i)<img[^>]+src=["']([^"']+)["']"#).expect("img src"));
    let texts: Vec<String> = text_re
        .captures_iter(smil_xml)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    let imgs: Vec<String> = img_re
        .captures_iter(smil_xml)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();
    (texts, imgs)
}

pub fn part_content_keys(part: &MmsPart) -> HashSet<String> {
    let mut keys = HashSet::new();
    for raw in [&part.name, &part.cl, &part.fn_attr] {
        let cleaned = raw.trim();
        if cleaned.is_empty() || cleaned.eq_ignore_ascii_case("null") || cleaned.eq_ignore_ascii_case("none")
        {
            continue;
        }
        keys.insert(cleaned.to_string());
        if let Some(base) = cleaned.rsplit('/').next() {
            if !base.is_empty() {
                keys.insert(base.to_string());
            }
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_smil_refs() {
        let smil = r#"<smil><body><par><text src="text_0.txt"/><img src="IMG_1.jpg"/></par></body></smil>"#;
        let (texts, imgs) = ordered_smil_refs(smil);
        assert_eq!(texts, vec!["text_0.txt"]);
        assert_eq!(imgs, vec!["IMG_1.jpg"]);
    }
}
