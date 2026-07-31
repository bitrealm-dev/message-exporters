//! Locate and copy iMazing attachment files next to CSV exports.

use anyhow::{Context, Result};
use chrono::{Local, TimeZone};
use message_csv::AttachmentCell;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Resolve a CSV attachment name into an [`AttachmentCell`].
///
/// Lookup order (unchanged):
/// 1. Files in the CSV's parent directory
/// 2. Recursive walk under `input_root`
///
/// When `copy_attachments` is false, keep the CSV name only. On copy failure or
/// a missing file, fall back to the CSV name so the row still projects.
pub(crate) fn resolve_attachment_cell(
    csv_name: &str,
    attachment_type: &str,
    csv_parent: &Path,
    input_root: &Path,
    attachments_dir: &Path,
    copy_attachments: bool,
    message_secs: i64,
    attachments_saved: &mut u64,
) -> AttachmentCell {
    let mime = mime_hint(attachment_type, csv_name);
    let is_sticker = attachment_type.eq_ignore_ascii_case("sticker");
    if !copy_attachments {
        return AttachmentCell {
            path: Some(csv_name.to_string()),
            original_name: Some(csv_name.to_string()),
            mime_type: mime,
            digest_sha256: None,
            is_sticker,
            transcription: None,
            sticker_effect: None,
        };
    }
    match find_and_copy_attachment(
        csv_name,
        csv_parent,
        input_root,
        attachments_dir,
        message_secs,
        attachments_saved,
    ) {
        Ok(Some(rel_path)) => AttachmentCell {
            path: Some(rel_path),
            original_name: Some(csv_name.to_string()),
            mime_type: mime,
            digest_sha256: None,
            is_sticker,
            transcription: None,
            sticker_effect: None,
        },
        Ok(None) | Err(_) => AttachmentCell {
            path: Some(csv_name.to_string()),
            original_name: Some(csv_name.to_string()),
            mime_type: mime,
            digest_sha256: None,
            is_sticker,
            transcription: None,
            sticker_effect: None,
        },
    }
}

fn attachment_name_matches(disk_name: &str, csv_name: &str) -> bool {
    let disk = disk_name.to_ascii_lowercase();
    let csv = csv_name.to_ascii_lowercase();
    if disk == csv {
        return true;
    }
    disk.ends_with(&csv) || disk.ends_with(&format!("_{csv}")) || disk.ends_with(&format!("-{csv}"))
}

fn find_attachment_on_disk(
    csv_name: &str,
    csv_parent: &Path,
    input_root: &Path,
) -> Option<PathBuf> {
    if let Ok(entries) = fs::read_dir(csv_parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && attachment_name_matches(name, csv_name)
            {
                return Some(path);
            }
        }
    }
    find_attachment_walk(csv_name, input_root)
}

fn find_attachment_walk(csv_name: &str, dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_attachment_walk(csv_name, &path) {
                return Some(found);
            }
            continue;
        }
        if !path.is_file() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && attachment_name_matches(name, csv_name)
        {
            return Some(path);
        }
    }
    None
}

fn find_and_copy_attachment(
    csv_name: &str,
    csv_parent: &Path,
    input_root: &Path,
    attachments_dir: &Path,
    message_secs: i64,
    attachments_saved: &mut u64,
) -> Result<Option<String>> {
    let Some(src) = find_attachment_on_disk(csv_name, csv_parent, input_root) else {
        return Ok(None);
    };
    let bytes = fs::read(&src).with_context(|| format!("read {}", src.display()))?;
    let digest_hex = hex::encode(Sha256::digest(&bytes));
    let digest_prefix = &digest_hex[..16.min(digest_hex.len())];
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let date_prefix = Local
        .timestamp_opt(message_secs, 0)
        .single()
        .map(|t| t.format("%Y%m%d_%H%M%S").to_string())
        .unwrap_or_else(|| message_secs.to_string());
    let name = format!("{date_prefix}-{digest_prefix}{ext}");
    let dest = attachments_dir.join(&name);
    if !dest.exists() {
        fs::write(&dest, &bytes).with_context(|| format!("write {}", dest.display()))?;
        *attachments_saved += 1;
    }
    Ok(Some(format!("attachments/{name}")))
}

fn mime_hint(attachment_type: &str, filename: &str) -> Option<String> {
    let t = attachment_type.trim().to_ascii_lowercase();
    if !t.is_empty() {
        return Some(match t.as_str() {
            "image" => "image/jpeg".into(),
            "video" => "video/mp4".into(),
            "audio" => "audio/mpeg".into(),
            "gif" => "image/gif".into(),
            "sticker" => "image/webp".into(),
            other => other.to_string(),
        });
    }
    let lower = filename.to_ascii_lowercase();
    if lower.ends_with(".png") {
        Some("image/png".into())
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some("image/jpeg".into())
    } else if lower.ends_with(".gif") {
        Some("image/gif".into())
    } else if lower.ends_with(".heic") {
        Some("image/heic".into())
    } else if lower.ends_with(".mp4") || lower.ends_with(".mov") {
        Some("video/mp4".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_name_matches_suffix_and_separators() {
        assert!(attachment_name_matches("IMG_1234.jpg", "1234.jpg"));
        assert!(attachment_name_matches("photo_abc.jpg", "abc.jpg"));
        assert!(attachment_name_matches("photo-abc.jpg", "abc.jpg"));
        assert!(!attachment_name_matches("other.jpg", "abc.jpg"));
    }
}
