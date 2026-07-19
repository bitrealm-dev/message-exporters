use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

/// Rewrite `attachments_json` paths in every `*.csv` under `output_dir` using `remap`
/// (export-relative paths with `/` separators).
pub fn rewrite_attachment_paths(
    output_dir: &Path,
    remap: &HashMap<String, String>,
) -> Result<usize> {
    if remap.is_empty() {
        return Ok(0);
    }
    let mut count = 0usize;
    let mut csv_paths: Vec<_> = fs::read_dir(output_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
        })
        .collect();
    csv_paths.sort();
    for path in csv_paths {
        if rewrite_one_csv(&path, remap)? {
            count += 1;
        }
    }
    Ok(count)
}

fn rewrite_one_csv(path: &Path, remap: &HashMap<String, String>) -> Result<bool> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(path)
        .with_context(|| format!("read {}", path.display()))?;
    let headers = rdr.headers()?.clone();
    let Some(att_idx) = headers.iter().position(|h| h == "attachments_json") else {
        return Ok(false);
    };
    let mut changed = false;
    let mut rows = Vec::new();
    for result in rdr.records() {
        let mut record = result?;
        let raw = record.get(att_idx).unwrap_or("").to_string();
        let rewritten = rewrite_attachments_json(&raw, remap);
        if rewritten != raw {
            changed = true;
            record = set_field(&record, att_idx, &rewritten);
        }
        rows.push(record);
    }
    if !changed {
        return Ok(false);
    }
    let tmp = path.with_extension("csv.tmp");
    {
        let mut wtr = csv::Writer::from_path(&tmp)?;
        wtr.write_record(&headers)?;
        for row in &rows {
            wtr.write_record(row)?;
        }
        wtr.flush()?;
    }
    fs::rename(&tmp, path)?;
    Ok(true)
}

fn set_field(record: &csv::StringRecord, idx: usize, value: &str) -> csv::StringRecord {
    let mut out = csv::StringRecord::new();
    for (i, field) in record.iter().enumerate() {
        if i == idx {
            out.push_field(value);
        } else {
            out.push_field(field);
        }
    }
    // pad if short
    while out.len() <= idx {
        if out.len() == idx {
            out.push_field(value);
        } else {
            out.push_field("");
        }
    }
    out
}

fn rewrite_attachments_json(raw: &str, remap: &HashMap<String, String>) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" || trimmed == "[]" {
        return trimmed.to_string();
    }
    let Ok(mut value) = serde_json::from_str::<Value>(trimmed) else {
        return raw.to_string();
    };
    let Some(arr) = value.as_array_mut() else {
        return raw.to_string();
    };
    let mut changed = false;
    for item in arr.iter_mut() {
        let Some(obj) = item.as_object_mut() else {
            continue;
        };
        let Some(path) = obj.get("path").and_then(|v| v.as_str()) else {
            continue;
        };
        let key = normalize_rel(path);
        if let Some(new_path) = remap.get(&key) {
            obj.insert("path".into(), Value::String(new_path.clone()));
            if let Some(mime) = mime_for_path(new_path) {
                obj.insert("mime_type".into(), Value::String(mime.into()));
            }
            changed = true;
        }
    }
    if changed {
        serde_json::to_string(&value).unwrap_or_else(|_| raw.to_string())
    } else {
        raw.to_string()
    }
}

fn normalize_rel(path: &str) -> String {
    path.replace('\\', "/")
}

fn mime_for_path(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "mp4" => Some("video/mp4"),
        "mp3" => Some("audio/mpeg"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_json_paths() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("chat.csv");
        let mut wtr = csv::Writer::from_path(&csv_path).unwrap();
        wtr.write_record(["chat_identifier", "attachments_json"])
            .unwrap();
        wtr.write_record([
            "+1555",
            r#"[{"path":"attachments/a.heic","mime_type":"image/heic","is_sticker":false}]"#,
        ])
        .unwrap();
        wtr.flush().unwrap();

        let mut remap = HashMap::new();
        remap.insert("attachments/a.heic".into(), "attachments/a.jpg".into());
        assert_eq!(rewrite_attachment_paths(dir.path(), &remap).unwrap(), 1);
        let text = fs::read_to_string(&csv_path).unwrap();
        assert!(text.contains("attachments/a.jpg"));
        assert!(text.contains("image/jpeg"));
    }
}
