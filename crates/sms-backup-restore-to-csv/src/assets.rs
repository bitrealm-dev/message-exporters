//! Extract base64 MMS part blobs from SMS Backup & Restore XML parts.

use crate::smil::part_content_keys;
use crate::xml::{AttachmentBlob, MmsPart, XmlParseStats};
use base64::Engine;
use chrono::TimeZone;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

fn valid_filename(name: Option<&str>) -> Option<String> {
    let cleaned = name?.trim();
    if cleaned.is_empty()
        || cleaned.eq_ignore_ascii_case("null")
        || cleaned.eq_ignore_ascii_case("none")
    {
        None
    } else {
        Some(cleaned.to_string())
    }
}

fn extension_for_part(part: &MmsPart) -> String {
    let ct = part.ct.to_ascii_lowercase();
    match ct.as_str() {
        "image/jpeg" | "image/jpg" => return ".jpg".into(),
        "image/png" => return ".png".into(),
        "image/gif" => return ".gif".into(),
        "image/webp" => return ".webp".into(),
        "video/mp4" => return ".mp4".into(),
        "video/3gpp" | "video/3gp" => return ".3gp".into(),
        "audio/amr" => return ".amr".into(),
        "audio/mpeg" => return ".mp3".into(),
        "audio/mp4" => return ".m4a".into(),
        _ => {}
    }
    for name in [&part.name, &part.cl, &part.fn_attr] {
        if let Some(valid) = valid_filename(Some(name)) {
            let suffix = Path::new(&valid)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| format!(".{e}"));
            if let Some(s) = suffix {
                return s.to_ascii_lowercase();
            }
        }
    }
    if ct.starts_with("image/") {
        ".jpg".into()
    } else if ct.starts_with("video/") {
        ".mp4".into()
    } else if ct.starts_with("audio/") {
        ".amr".into()
    } else {
        ".bin".into()
    }
}

fn safe_basename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(|c| c == '.' || c == '_');
    if trimmed.is_empty() {
        "attachment".into()
    } else {
        trimmed.to_string()
    }
}

fn mime_for_ext(ext: &str) -> Option<&'static str> {
    match ext {
        ".jpg" | ".jpeg" => Some("image/jpeg"),
        ".png" => Some("image/png"),
        ".gif" => Some("image/gif"),
        ".webp" => Some("image/webp"),
        ".mp4" => Some("video/mp4"),
        ".3gp" => Some("video/3gpp"),
        ".amr" => Some("audio/amr"),
        ".mp3" => Some("audio/mpeg"),
        ".m4a" => Some("audio/mp4"),
        _ => None,
    }
}

/// Decode embedded base64 MMS parts. Filenames include a content digest so
/// identical bytes share a path and different bytes never collide.
pub fn extract_mms_attachments(
    parts: &[MmsPart],
    timestamp_ms: f64,
    img_refs: &[String],
    stats: &mut XmlParseStats,
) -> Vec<AttachmentBlob> {
    let date_prefix = chrono::Local
        .timestamp_opt((timestamp_ms / 1000.0) as i64, 0)
        .single()
        .map(|t| t.format("%Y%m%d_%H%M%S").to_string())
        .unwrap_or_else(|| "unknown".into());

    let mut by_key: HashMap<String, AttachmentBlob> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for part in parts {
        let ct = part.ct.to_ascii_lowercase();
        if ct.starts_with("text/") || ct == "application/smil" {
            continue;
        }
        let data_b64 = part.data.trim();
        if data_b64.is_empty() || data_b64.eq_ignore_ascii_case("null") {
            continue;
        }
        let payload = match base64::engine::general_purpose::STANDARD.decode(data_b64) {
            Ok(p) if !p.is_empty() => p,
            _ => {
                stats.skipped_bad_attachment += 1;
                continue;
            }
        };

        let digest_hex = hex::encode(Sha256::digest(&payload));
        let data: Arc<[u8]> = Arc::from(payload.into_boxed_slice());
        let ext = extension_for_part(part);
        let original = valid_filename(Some(&part.name))
            .or_else(|| valid_filename(Some(&part.cl)))
            .or_else(|| valid_filename(Some(&part.fn_attr)));
        // Content-addressed: short digest prefix makes collisions impossible.
        let digest_prefix = &digest_hex[..16];
        let out_name = if let Some(ref orig) = original {
            format!(
                "{date_prefix}_{digest_prefix}_{}",
                safe_basename(orig)
            )
        } else {
            format!("{date_prefix}_{digest_prefix}{ext}")
        };

        let blob = AttachmentBlob {
            filename: out_name.clone(),
            original_name: original,
            mime_type: mime_for_ext(&ext)
                .map(|s| s.to_string())
                .or_else(|| {
                    let c = part.ct.trim();
                    if c.is_empty() {
                        None
                    } else {
                        Some(c.to_string())
                    }
                }),
            data,
            digest_hex,
        };

        let keys = part_content_keys(part);
        if keys.is_empty() {
            by_key.insert(out_name.clone(), blob);
            order.push(out_name);
        } else {
            let mut first = true;
            for key in keys {
                if first {
                    order.push(key.clone());
                    first = false;
                }
                // Arc makes this clone cheap (shared bytes).
                by_key.entry(key).or_insert_with(|| blob.clone());
            }
        }
    }

    fn all_parts(
        by_key: &HashMap<String, AttachmentBlob>,
        order: &[String],
    ) -> Vec<AttachmentBlob> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for key in order {
            if let Some(blob) = by_key.get(key) {
                if seen.insert(blob.filename.clone()) {
                    out.push(blob.clone());
                }
            }
        }
        for blob in by_key.values() {
            if seen.insert(blob.filename.clone()) {
                out.push(blob.clone());
            }
        }
        out
    }

    if img_refs.is_empty() {
        return all_parts(&by_key, &order);
    }

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for r in img_refs {
        if let Some(blob) = by_key.get(r) {
            if seen.insert(blob.filename.clone()) {
                out.push(blob.clone());
            }
        }
    }
    // Odd SMIL `src` names that don't key-match parts: fall back to every media part.
    if out.is_empty() && !by_key.is_empty() {
        return all_parts(&by_key, &order);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::MmsPart;

    #[test]
    fn decodes_tiny_jpeg() {
        let data = base64::engine::general_purpose::STANDARD.encode(b"\xff\xd8\xfffakejpegdata!!!");
        let parts = vec![MmsPart {
            ct: "image/jpeg".into(),
            name: "pic.jpg".into(),
            data,
            ..Default::default()
        }];
        let mut stats = XmlParseStats::default();
        let out = extract_mms_attachments(&parts, 1_609_459_200_000.0, &[], &mut stats);
        assert_eq!(out.len(), 1);
        assert!(
            out[0].filename.contains("pic.jpg")
                || out[0].original_name.as_deref() == Some("pic.jpg")
        );
        assert_eq!(out[0].mime_type.as_deref(), Some("image/jpeg"));
        assert_eq!(out[0].digest_hex.len(), 64);
    }

    #[test]
    fn same_bytes_share_digest_filename() {
        let data = base64::engine::general_purpose::STANDARD.encode(b"same-bytes");
        let parts = vec![
            MmsPart {
                ct: "image/jpeg".into(),
                name: "a.jpg".into(),
                data: data.clone(),
                ..Default::default()
            },
            MmsPart {
                ct: "image/jpeg".into(),
                name: "b.jpg".into(),
                data,
                ..Default::default()
            },
        ];
        let mut stats = XmlParseStats::default();
        let out = extract_mms_attachments(&parts, 1_609_459_200_000.0, &[], &mut stats);
        assert_eq!(out.len(), 2);
        let prefix_a = out[0].digest_hex[..16].to_string();
        let prefix_b = out[1].digest_hex[..16].to_string();
        assert_eq!(prefix_a, prefix_b);
        assert!(out[0].filename.contains(&prefix_a));
        assert!(out[1].filename.contains(&prefix_b));
    }

    #[test]
    fn smil_mismatch_falls_back_to_all_parts() {
        let data = base64::engine::general_purpose::STANDARD.encode(b"\xff\xd8\xffjpeg");
        let parts = vec![MmsPart {
            ct: "image/jpeg".into(),
            name: "IMG_001.jpg".into(),
            data,
            ..Default::default()
        }];
        let mut stats = XmlParseStats::default();
        let out = extract_mms_attachments(
            &parts,
            1_609_459_200_000.0,
            &["cid:totally-unrelated@smil".into()],
            &mut stats,
        );
        assert_eq!(out.len(), 1);
        assert!(
            out[0]
                .original_name
                .as_deref()
                .is_some_and(|n| n.contains("IMG_001"))
        );
    }
}
