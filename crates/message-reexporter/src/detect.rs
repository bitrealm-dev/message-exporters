//! Auto-detect IR export layout in an input directory.

use anyhow::{bail, Context, Result};
use message_exporters_core::OutputFormat;
use message_ir::CSV_HEADERS;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Detected exclusive input packaging for a Message Exporters output directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectedExport {
    pub format: OutputFormat,
}

/// Auto-detect the IR format present in `input_dir` (top-level only).
///
/// Requires exactly one format class. Ignores `attachments/`, `*.meta.json`, and temps.
pub fn detect_ir_export(input_dir: &Path) -> Result<DetectedExport> {
    if !input_dir.is_dir() {
        bail!("input is not a directory: {}", input_dir.display());
    }

    let mut has_xml = false;
    let mut has_json = false;
    let mut has_jsonl = false;
    let mut has_csv = false;
    let mut has_mbox = false;
    let mut has_eml = false;
    let mut samples: Vec<String> = Vec::new();

    for entry in fs::read_dir(input_dir)
        .with_context(|| format!("read {}", input_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "attachments" || name.starts_with('.') {
            continue;
        }
        if name.ends_with(".meta.json")
            || name.ends_with(".tmp")
            || name.ends_with(".xml.tmp")
            || name.ends_with(".xml.sbrbody")
        {
            continue;
        }

        if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            match ext.as_str() {
                "xml" if name.eq_ignore_ascii_case("smses.xml") || looks_like_smses(&path) => {
                    has_xml = true;
                    samples.push(name);
                }
                "json" => {
                    if looks_like_ir_json(&path)? {
                        has_json = true;
                        samples.push(name);
                    }
                }
                "jsonl" | "ndjson" => {
                    if looks_like_ir_jsonl(&path)? {
                        has_jsonl = true;
                        samples.push(name);
                    }
                }
                "csv" => {
                    if looks_like_ir_csv(&path)? {
                        has_csv = true;
                        samples.push(name);
                    }
                }
                "mbox" => {
                    has_mbox = true;
                    samples.push(name);
                }
                _ => {}
            }
        } else if path.is_dir() && dir_has_eml(&path)? {
            has_eml = true;
            samples.push(format!("{name}/"));
        }
    }

    let flags = [
        (has_xml, OutputFormat::Xml, "xml"),
        (has_json, OutputFormat::Json, "json"),
        (has_jsonl, OutputFormat::Jsonl, "jsonl"),
        (has_csv, OutputFormat::Csv, "csv"),
        (has_mbox, OutputFormat::Mbox, "mbox"),
        (has_eml, OutputFormat::Eml, "eml"),
    ];
    let present: Vec<_> = flags
        .iter()
        .filter(|(on, _, _)| *on)
        .map(|(_, fmt, label)| (*fmt, *label))
        .collect();

    match present.as_slice() {
        [(format, _)] => Ok(DetectedExport { format: *format }),
        [] => bail!(
            "unsupported input: no Message Exporters IR export found in {} \
             (expected smses.xml, *.json, *.jsonl, *.csv, *.mbox, or EML folders)",
            input_dir.display()
        ),
        many => {
            let kinds: Vec<&str> = many.iter().map(|(_, l)| *l).collect();
            bail!(
                "unsupported input: mixed formats in {} ({}); found: {}",
                input_dir.display(),
                kinds.join(", "),
                samples.join(", ")
            );
        }
    }
}

fn looks_like_smses(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut buf = String::new();
    let mut reader = BufReader::new(file);
    let _ = reader.read_line(&mut buf);
    buf.to_ascii_lowercase().contains("<smses")
}

fn looks_like_ir_json(path: &Path) -> Result<bool> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let v: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    Ok(v.get("schema_version").and_then(|x| x.as_u64()) == Some(3)
        && v.get("export").is_some()
        && v.get("conversation").is_some()
        && v.get("messages").is_some())
}

fn looks_like_ir_jsonl(path: &Path) -> Result<bool> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut lines = BufReader::new(file).lines();
    let Some(Ok(first)) = lines.next() else {
        return Ok(false);
    };
    let v: serde_json::Value = match serde_json::from_str(&first) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    Ok(v.get("schema_version").and_then(|x| x.as_u64()) == Some(3)
        && v.get("export").is_some()
        && v.get("conversation").is_some()
        && v.get("messages").is_none())
}

fn looks_like_ir_csv(path: &Path) -> Result<bool> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(BufReader::new(file));
    let Ok(headers) = rdr.headers() else {
        return Ok(false);
    };
    let set: std::collections::HashSet<&str> = headers.iter().collect();
    Ok(CSV_HEADERS.iter().all(|h| set.contains(h)))
}

fn dir_has_eml(dir: &Path) -> Result<bool> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("eml"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// List conversation artifacts for a detected format.
pub fn list_artifacts(input_dir: &Path, format: OutputFormat) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(input_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "attachments" || name.starts_with('.') || name.ends_with(".meta.json") {
            continue;
        }
        match format {
            OutputFormat::Xml => {
                if path.is_file()
                    && (name.eq_ignore_ascii_case("smses.xml") || looks_like_smses(&path))
                {
                    paths.push(path);
                }
            }
            OutputFormat::Json => {
                if path.is_file()
                    && path.extension().and_then(|e| e.to_str()) == Some("json")
                    && looks_like_ir_json(&path)?
                {
                    paths.push(path);
                }
            }
            OutputFormat::Jsonl => {
                if path.is_file()
                    && path
                        .extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e == "jsonl" || e == "ndjson")
                    && looks_like_ir_jsonl(&path)?
                {
                    paths.push(path);
                }
            }
            OutputFormat::Csv => {
                if path.is_file()
                    && path.extension().and_then(|e| e.to_str()) == Some("csv")
                    && looks_like_ir_csv(&path)?
                {
                    paths.push(path);
                }
            }
            OutputFormat::Mbox => {
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("mbox") {
                    paths.push(path);
                }
            }
            OutputFormat::Eml => {
                if path.is_dir() && dir_has_eml(&path)? {
                    paths.push(path);
                }
            }
        }
    }
    paths.sort();
    if paths.is_empty() {
        bail!(
            "no {} artifacts found in {}",
            format.as_str(),
            input_dir.display()
        );
    }
    Ok(paths)
}
