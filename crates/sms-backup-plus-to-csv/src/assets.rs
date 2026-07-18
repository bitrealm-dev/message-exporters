//! Extract MIME attachment blobs from SMS Backup+ EML messages.

use crate::types::AttachmentBlob;
use mailparse::{MailHeaderMap, ParsedMail};
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

static SAFE_RE: OnceLock<Regex> = OnceLock::new();

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

fn extension_for(ctype: &str, filename: Option<&str>) -> String {
    let ct = ctype.to_ascii_lowercase();
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
    if let Some(valid) = valid_filename(filename)
        && let Some(ext) = Path::new(&valid).extension().and_then(|e| e.to_str())
    {
        return format!(".{}", ext.to_ascii_lowercase());
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
    let re = SAFE_RE.get_or_init(|| Regex::new(r"[^\w.\-]+").expect("safe"));
    let cleaned = re.replace_all(name, "_");
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

fn walk_parts<'a>(mail: &'a ParsedMail<'a>, out: &mut Vec<&'a ParsedMail<'a>>) {
    if mail.subparts.is_empty() {
        out.push(mail);
    } else {
        for part in &mail.subparts {
            walk_parts(part, out);
        }
    }
}

/// Decode non-text MIME parts into attachment blobs.
pub(crate) fn extract_attachments(
    mail: &ParsedMail<'_>,
    timestamp_ms: f64,
    file_key: Option<&str>,
) -> Vec<AttachmentBlob> {
    let date_prefix = crate::identity::local_datetime_from_secs((timestamp_ms / 1000.0) as i64)
        .format("%Y%m%d_%H%M%S")
        .to_string();
    let ts_int = timestamp_ms as i64;
    let name_prefix = file_key.map(|k| format!("{k}_")).unwrap_or_default();

    let mut parts = Vec::new();
    walk_parts(mail, &mut parts);

    let mut out = Vec::new();
    let mut seq = 0u32;
    for part in parts {
        let ctype = part.ctype.mimetype.to_ascii_lowercase();
        if ctype.starts_with("multipart/") || ctype.starts_with("text/") {
            continue;
        }
        let payload = match part.get_body_raw() {
            Ok(p) if !p.is_empty() => p,
            _ => continue,
        };
        seq += 1;
        let filename = part
            .get_content_disposition()
            .params
            .get("filename")
            .cloned()
            .or_else(|| part.headers.get_first_value("Content-Type").and(None));
        // Prefer Content-Disposition filename
        let original = valid_filename(filename.as_deref()).or_else(|| {
            part.get_content_disposition()
                .params
                .get("name")
                .and_then(|n| valid_filename(Some(n)))
        });
        let ext = extension_for(&ctype, original.as_deref());
        let out_name = if let Some(ref orig) = original {
            format!(
                "{name_prefix}{date_prefix}_{ts_int}_{seq}_{}",
                safe_basename(orig)
            )
        } else {
            format!("{name_prefix}{date_prefix}_{ts_int}_{seq}{ext}")
        };
        out.push(AttachmentBlob {
            filename: out_name,
            original_name: original,
            mime_type: mime_for_ext(&ext)
                .map(|s| s.to_string())
                .or(if ctype.is_empty() { None } else { Some(ctype) }),
            data: payload,
        });
    }
    out
}
