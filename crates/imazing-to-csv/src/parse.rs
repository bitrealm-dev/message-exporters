//! Parse iMazing Messages CSV exports.

use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct RawRow {
    pub chat_session: String,
    pub message_date: String,
    pub service: String,
    pub msg_type: String,
    pub sender_id: String,
    pub sender_name: String,
    pub subject: String,
    pub text: String,
    pub reactions: String,
    pub status: String,
    pub attachment: String,
    pub attachment_type: String,
}

/// Discover iMazing Messages CSV files under `input` (file or directory).
pub(crate) fn discover_csv_files(input: &Path) -> Result<Vec<PathBuf>> {
    if input.is_file() {
        if looks_like_imazing_messages(input)? {
            return Ok(vec![input.to_path_buf()]);
        }
        bail!(
            "{} is not an iMazing Messages CSV (need Chat Session + Message Date + Sender ID)",
            input.display()
        );
    }
    if !input.is_dir() {
        bail!("input path not found: {}", input.display());
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(input)
        .with_context(|| format!("read_dir {}", input.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with(".csv") {
            continue;
        }
        // Skip Contacts exports and attachment sidecars by name.
        let lower = name.to_ascii_lowercase();
        if lower.starts_with("contacts") || lower.contains("attachment") {
            continue;
        }
        if looks_like_imazing_messages(&path)? {
            files.push(path);
        }
    }
    files.sort();
    if files.is_empty() {
        bail!(
            "no iMazing Messages CSV files found under {}",
            input.display()
        );
    }
    Ok(files)
}

fn looks_like_imazing_messages(path: &Path) -> Result<bool> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut buf = vec![0u8; 4096];
    let n = file.read(&mut buf)?;
    let text = String::from_utf8_lossy(&buf[..n]);
    let header = text.lines().next().unwrap_or("").trim_start_matches('\u{feff}');
    let lower = header.to_ascii_lowercase();
    Ok(lower.contains("chat session")
        && lower.contains("message date")
        && lower.contains("sender id"))
}

pub(crate) fn parse_csv_file(path: &Path) -> Result<Vec<RawRow>> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("read {}", path.display()))?;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.drain(..3);
    }
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(bytes.as_slice());
    let headers = rdr
        .headers()
        .with_context(|| format!("headers {}", path.display()))?
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();

    let chat_i = col(&headers, "chat session")?;
    let date_i = col(&headers, "message date")?;
    let type_i = col(&headers, "type")?;
    let sender_id_i = col(&headers, "sender id")?;
    let sender_name_i = col(&headers, "sender name")?;
    let text_i = col(&headers, "text")?;
    let service_i = headers.iter().position(|h| h == "service");
    let subject_i = headers.iter().position(|h| h == "subject");
    let reactions_i = headers.iter().position(|h| h == "reactions");
    let status_i = headers.iter().position(|h| h == "status");
    let att_i = headers.iter().position(|h| h == "attachment");
    let att_type_i = headers.iter().position(|h| h == "attachment type");

    let mut rows = Vec::new();
    for (idx, rec) in rdr.records().enumerate() {
        let rec = rec.with_context(|| format!("row {} in {}", idx + 2, path.display()))?;
        let chat_session = field(&rec, chat_i);
        let message_date = field(&rec, date_i);
        let text = field(&rec, text_i);
        let attachment = att_i.map(|i| field(&rec, i)).unwrap_or_default();
        if chat_session.is_empty()
            && message_date.is_empty()
            && text.is_empty()
            && attachment.is_empty()
        {
            continue;
        }
        rows.push(RawRow {
            chat_session,
            message_date,
            service: service_i.map(|i| field(&rec, i)).unwrap_or_default(),
            msg_type: field(&rec, type_i),
            sender_id: field(&rec, sender_id_i),
            sender_name: field(&rec, sender_name_i),
            subject: subject_i.map(|i| field(&rec, i)).unwrap_or_default(),
            text,
            reactions: reactions_i.map(|i| field(&rec, i)).unwrap_or_default(),
            status: status_i.map(|i| field(&rec, i)).unwrap_or_default(),
            attachment,
            attachment_type: att_type_i.map(|i| field(&rec, i)).unwrap_or_default(),
        });
    }
    Ok(rows)
}

fn col(headers: &[String], name: &str) -> Result<usize> {
    headers
        .iter()
        .position(|h| h == name)
        .with_context(|| format!("missing column {name:?} (have {headers:?})"))
}

fn field(rec: &csv::StringRecord, idx: usize) -> String {
    rec.get(idx).unwrap_or("").trim().to_string()
}
