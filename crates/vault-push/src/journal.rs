//! Append-only resumable outcome journal under the export folder.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const JOURNAL_NAME: &str = ".vault-import-state.jsonl";
pub const REPORT_NAME: &str = "vault-push-report.json";
pub const LOG_NAME: &str = "vault-push.log";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum JournalEvent {
    AssetOk {
        url: String,
        username: String,
        source: String,
        sha256: String,
    },
    MessageOk {
        url: String,
        username: String,
        source: String,
        file: String,
        guid: String,
    },
    FileOk {
        url: String,
        username: String,
        source: String,
        file: String,
    },
    Fail {
        url: String,
        username: String,
        source: String,
        file: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        guid: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
        stage: String,
        error: String,
    },
}

#[derive(Debug, Default)]
pub struct JournalState {
    pub assets: HashSet<String>,
    pub messages: HashSet<String>,
    pub files: HashSet<String>,
}

impl JournalState {
    pub fn message_key(file: &str, guid: &str) -> String {
        format!("{file}\0{guid}")
    }
}

pub fn journal_path(input: &Path) -> PathBuf {
    input.join(JOURNAL_NAME)
}

pub fn load(path: &Path, url: &str, username: &str) -> Result<JournalState> {
    let mut state = JournalState::default();
    if !path.is_file() {
        return Ok(state);
    }
    let file = File::open(path).with_context(|| format!("open journal {}", path.display()))?;
    for (i, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("read journal line {}", i + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: JournalEvent = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };
        match event {
            JournalEvent::AssetOk {
                url: u,
                username: a,
                sha256,
                ..
            } if u == url && a == username => {
                state.assets.insert(sha256);
            }
            JournalEvent::MessageOk {
                url: u,
                username: a,
                file,
                guid,
                ..
            } if u == url && a == username => {
                state
                    .messages
                    .insert(JournalState::message_key(&file, &guid));
            }
            JournalEvent::FileOk {
                url: u,
                username: a,
                file,
                ..
            } if u == url && a == username => {
                state.files.insert(file);
            }
            _ => {}
        }
    }
    Ok(state)
}

pub fn append(path: &Path, event: &JournalEvent) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open journal for append {}", path.display()))?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

pub fn compact(path: &Path, url: &str, username: &str, state: &JournalState) -> Result<()> {
    let mut events = Vec::new();
    for sha in &state.assets {
        events.push(JournalEvent::AssetOk {
            url: url.to_string(),
            username: username.to_string(),
            source: String::new(),
            sha256: sha.clone(),
        });
    }
    for key in &state.messages {
        let Some((file, guid)) = key.split_once('\0') else {
            continue;
        };
        events.push(JournalEvent::MessageOk {
            url: url.to_string(),
            username: username.to_string(),
            source: String::new(),
            file: file.to_string(),
            guid: guid.to_string(),
        });
    }
    for file in &state.files {
        events.push(JournalEvent::FileOk {
            url: url.to_string(),
            username: username.to_string(),
            source: String::new(),
            file: file.clone(),
        });
    }
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut out = File::create(&tmp)?;
        for event in events {
            serde_json::to_writer(&mut out, &event)?;
            out.write_all(b"\n")?;
        }
    }
    fs::rename(&tmp, path)?;
    Ok(())
}
