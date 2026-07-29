//! Folder push: stream JSONL, upload assets by digest, import message batches.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use message_exporters_core::{CancelFlag, check_cancel};
use message_ir::{ConversationHeader, read_conversation_jsonl};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::http::{self, AuthInfo};
use crate::journal::{self, JournalEvent, JournalState};
use crate::project;

/// Default messages per HTTP import request.
pub const DEFAULT_BATCH_SIZE: usize = 100;
/// Default number of simultaneous attachment uploads.
pub const DEFAULT_ASSET_UPLOAD_WORKERS: usize = 4;

#[derive(Debug, Clone)]
pub struct VaultPushConfig {
    pub input: PathBuf,
    pub base_url: String,
    pub username: String,
    pub key: String,
    /// "append" (default) or "replace"
    pub mode: String,
    pub continue_on_error: bool,
    pub force: bool,
    pub max_retries: u32,
    pub batch_size: usize,
    /// Maximum simultaneous attachment uploads. Message import requests remain serialized.
    pub asset_upload_workers: usize,
    pub report_path: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    pub journal_path: Option<PathBuf>,
    pub cancel: Option<CancelFlag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileResult {
    pub file: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub messages: u64,
    pub attachments: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushReport {
    pub ok: bool,
    pub account: String,
    pub username: String,
    pub mode: String,
    pub started_at: String,
    pub finished_at: String,
    pub conversations_total: u64,
    pub conversations_ok: u64,
    pub conversations_failed: u64,
    pub conversations_skipped: u64,
    pub messages: u64,
    pub assets_uploaded: u64,
    pub assets_skipped: u64,
    pub results: Vec<FileResult>,
}

#[derive(Debug, Clone)]
pub enum ProgressEvent {
    Log(String),
    Auth {
        account_id: String,
        username: String,
    },
    FileStart {
        index: usize,
        total: usize,
        file: String,
    },
    FileDone {
        file: String,
        status: String,
    },
    Finished(PushReport),
}

pub type ProgressFn<'a> = dyn FnMut(ProgressEvent) + Send + 'a;

struct LogWriter {
    file: File,
}

impl LogWriter {
    fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("open log {}", path.display()))?;
        Ok(Self { file })
    }

    fn line(&mut self, msg: &str) {
        let _ = writeln!(self.file, "{msg}");
        let _ = self.file.flush();
    }
}

fn now_stamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn is_push_artifact(name: &str) -> bool {
    name.eq_ignore_ascii_case(journal::JOURNAL_NAME)
        || name.eq_ignore_ascii_case(journal::REPORT_NAME)
        || name.eq_ignore_ascii_case(journal::LOG_NAME)
        || name.ends_with(".jsonl.tmp")
        || name.starts_with('.')
}

fn list_jsonl_files(dir: &Path, exclude: &[&Path]) -> Result<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("read {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            if exclude.iter().any(|x| *x == p) {
                return false;
            }
            let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
                return false;
            };
            if is_push_artifact(name) {
                return false;
            }
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jsonl"))
        })
        .collect();
    paths.sort();
    Ok(paths)
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1024 * 64];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn resolve_attachment(export_root: &Path, rel: &str) -> Option<PathBuf> {
    let candidate = Path::new(rel);
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    let under = export_root.join(candidate);
    under.is_file().then_some(under)
}

fn safe_rel(rel: &str) -> Result<()> {
    let path = Path::new(rel);
    if path.is_absolute() {
        bail!("attachment path must be relative: {rel}");
    }
    for comp in path.components() {
        if matches!(comp, std::path::Component::ParentDir) {
            bail!("unsafe attachment path: {rel}");
        }
    }
    Ok(())
}

/// Authenticate without importing.
pub fn authenticate(base_url: &str, key: &str, username: &str) -> Result<AuthInfo> {
    http::auth_check(base_url, key, username)
}

/// Peek `export.source` from the first JSONL header in a directory.
pub fn detect_source(input: &Path) -> Result<Option<String>> {
    let dir = if input.is_file() {
        input.parent().unwrap_or(input)
    } else {
        input
    };
    let files = list_jsonl_files(dir, &[])?;
    let Some(path) = files.first() else {
        return Ok(None);
    };
    let file = File::open(path)?;
    let mut lines = BufReader::new(file).lines();
    let header_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty JSONL"))??;
    let header: ConversationHeader = serde_json::from_str(&header_line)?;
    Ok(Some(project::validate_header(&header)?))
}

/// Push every `.jsonl` conversation under `cfg.input`.
pub fn run(cfg: &VaultPushConfig, mut progress: Option<&mut ProgressFn<'_>>) -> Result<PushReport> {
    let started_at = now_stamp();
    let input = if cfg.input.is_file() {
        cfg.input
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        cfg.input.clone()
    };
    if !input.is_dir() {
        bail!("input directory does not exist: {}", input.display());
    }

    let report_path = cfg
        .report_path
        .clone()
        .unwrap_or_else(|| input.join(journal::REPORT_NAME));
    let log_path = cfg
        .log_path
        .clone()
        .unwrap_or_else(|| input.join(journal::LOG_NAME));
    let journal_path = cfg
        .journal_path
        .clone()
        .unwrap_or_else(|| journal::journal_path(&input));

    let mut log = LogWriter::open(&log_path)?;
    let url = cfg.base_url.trim_end_matches('/').to_string();
    let username = cfg.username.trim().to_string();

    check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
    let auth = http::auth_check(&url, &cfg.key, &username)?;
    let account_label = auth
        .username
        .clone()
        .unwrap_or_else(|| auth.account_id.clone());
    log.line(&format!(
        "authenticated username={username} account={}",
        auth.account_id
    ));
    if let Some(cb) = progress.as_mut() {
        cb(ProgressEvent::Auth {
            account_id: auth.account_id.clone(),
            username: account_label.clone(),
        });
        cb(ProgressEvent::Log(format!(
            "Authenticated as {account_label}"
        )));
    }

    let mut journal = if cfg.force || cfg.mode == "replace" {
        JournalState::default()
    } else {
        journal::load(&journal_path, &url, &username)?
    };

    let files = list_jsonl_files(&input, &[&journal_path, &report_path, &log_path])?;
    if files.is_empty() {
        bail!(
            "no .jsonl files under {} (export with JSONL in the Message tab first)",
            input.display()
        );
    }

    let total = files.len();
    let mut results = Vec::new();
    let mut ok_n = 0u64;
    let mut fail_n = 0u64;
    let mut skip_n = 0u64;
    let mut messages = 0u64;
    let mut assets_uploaded = 0u64;
    let mut assets_skipped = 0u64;
    let mut first_import = true;
    let mut aborted = false;
    let batch_size = cfg.batch_size.max(1);

    for (idx, path) in files.iter().enumerate() {
        check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        if let Some(cb) = progress.as_mut() {
            cb(ProgressEvent::FileStart {
                index: idx + 1,
                total,
                file: name.clone(),
            });
        }

        if cfg.mode == "append" && !cfg.force && journal.files.contains(&name) {
            skip_n += 1;
            let msg = format!(
                "PROGRESS {}/{total} skip {name} (already imported)",
                idx + 1
            );
            log.line(&msg);
            if let Some(cb) = progress.as_mut() {
                cb(ProgressEvent::Log(msg));
                cb(ProgressEvent::FileDone {
                    file: name.clone(),
                    status: "skipped".into(),
                });
            }
            results.push(FileResult {
                file: name,
                status: "skipped".into(),
                error: None,
                messages: 0,
                attachments: 0,
            });
            continue;
        }

        match push_one_file(PushFileArgs {
            input: &input,
            path,
            name: &name,
            cfg,
            url: &url,
            username: &username,
            journal: &mut journal,
            journal_path: &journal_path,
            batch_size,
            first_import: &mut first_import,
            assets_uploaded: &mut assets_uploaded,
            assets_skipped: &mut assets_skipped,
            log: &mut log,
        }) {
            Ok((count, atts)) => {
                ok_n += 1;
                messages += count;
                let msg = format!(
                    "PROGRESS {}/{total} ok {name} msgs={count} attachments={atts}",
                    idx + 1
                );
                log.line(&msg);
                if let Some(cb) = progress.as_mut() {
                    cb(ProgressEvent::Log(msg));
                    cb(ProgressEvent::FileDone {
                        file: name.clone(),
                        status: "ok".into(),
                    });
                }
                results.push(FileResult {
                    file: name,
                    status: "ok".into(),
                    error: None,
                    messages: count,
                    attachments: atts,
                });
            }
            Err(e) => {
                fail_n += 1;
                let err = e.to_string();
                let msg = format!("PROGRESS {}/{total} fail {name} {err}", idx + 1);
                log.line(&msg);
                let _ = journal::append(
                    &journal_path,
                    &JournalEvent::Fail {
                        url: url.clone(),
                        username: username.clone(),
                        source: String::new(),
                        file: name.clone(),
                        guid: None,
                        sha256: None,
                        stage: "file".into(),
                        error: err.clone(),
                    },
                );
                if let Some(cb) = progress.as_mut() {
                    cb(ProgressEvent::Log(msg));
                    cb(ProgressEvent::FileDone {
                        file: name.clone(),
                        status: "failed".into(),
                    });
                }
                results.push(FileResult {
                    file: name,
                    status: "failed".into(),
                    error: Some(err),
                    messages: 0,
                    attachments: 0,
                });
                if !cfg.continue_on_error {
                    aborted = true;
                    break;
                }
            }
        }
    }

    if fail_n == 0 && !aborted {
        let _ = journal::compact(&journal_path, &url, &username, &journal);
    }

    let report = PushReport {
        ok: fail_n == 0 && !aborted,
        account: auth.account_id,
        username,
        mode: cfg.mode.clone(),
        started_at,
        finished_at: now_stamp(),
        conversations_total: total as u64,
        conversations_ok: ok_n,
        conversations_failed: fail_n,
        conversations_skipped: skip_n,
        messages,
        assets_uploaded,
        assets_skipped,
        results,
    };
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &report_path,
        serde_json::to_string_pretty(&report).context("serialize report")?,
    )
    .with_context(|| format!("write report {}", report_path.display()))?;
    log.line(&format!(
        "finished ok={} conversations_ok={ok_n} failed={fail_n} skipped={skip_n} messages={messages}",
        report.ok
    ));
    if let Some(cb) = progress.as_mut() {
        cb(ProgressEvent::Finished(report.clone()));
    }
    Ok(report)
}

struct PushFileArgs<'a> {
    input: &'a Path,
    path: &'a Path,
    name: &'a str,
    cfg: &'a VaultPushConfig,
    url: &'a str,
    username: &'a str,
    journal: &'a mut JournalState,
    journal_path: &'a Path,
    batch_size: usize,
    first_import: &'a mut bool,
    assets_uploaded: &'a mut u64,
    assets_skipped: &'a mut u64,
    log: &'a mut LogWriter,
}

fn push_one_file(args: PushFileArgs<'_>) -> Result<(u64, u64)> {
    let PushFileArgs {
        input,
        path,
        name,
        cfg,
        url,
        username,
        journal,
        journal_path,
        batch_size,
        first_import,
        assets_uploaded,
        assets_skipped,
        log,
    } = args;

    let doc = read_conversation_jsonl(path)?;
    let header = ConversationHeader::from_document(&doc);
    let source = project::validate_header(&header)?;
    let messages = &doc.messages;

    let mut per_message_digests: Vec<Vec<(usize, String)>> = Vec::with_capacity(messages.len());
    // digest -> (rel path, mime)
    let mut unique: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();
    let mut attachment_count = 0u64;

    for msg in messages {
        let mut digests = Vec::new();
        for (att_i, att) in msg.attachments.iter().enumerate() {
            attachment_count += 1;
            let Some(rel) = att.path.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
                bail!("{name}: attachment {att_i} has no path");
            };
            safe_rel(rel)?;
            let abs = resolve_attachment(input, rel)
                .ok_or_else(|| anyhow::anyhow!("{name}: missing attachment {rel}"))?;
            let digest = match att
                .digest_sha256
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                Some(d) => {
                    let actual = hash_file(&abs)?;
                    if actual != d.to_ascii_lowercase() {
                        bail!("{name}: sha256 mismatch for {rel}");
                    }
                    actual
                }
                None => hash_file(&abs)?,
            };
            unique
                .entry(digest.clone())
                .or_insert_with(|| (rel.to_string(), att.mime_type.clone()));
            digests.push((att_i, digest));
        }
        per_message_digests.push(digests);
    }

    upload_assets(UploadAssets {
        input,
        name,
        cfg,
        url,
        username,
        source: &source,
        unique: &unique,
        journal,
        journal_path,
        assets_uploaded,
        assets_skipped,
        log,
    })?;

    let header_line = project::document_conversation_line(&doc)?;
    let mut pending = header_line.clone();
    let mut pending_guids: Vec<String> = Vec::new();
    let mut imported = 0u64;
    let mut batch_i = 0usize;

    for (i, msg) in messages.iter().enumerate() {
        check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
        let (line, guid) = project::message_line(msg, &per_message_digests[i])?;
        if !cfg.force
            && journal
                .messages
                .contains(&JournalState::message_key(name, &guid))
        {
            continue;
        }
        pending.extend_from_slice(&line);
        pending_guids.push(guid);
        batch_i += 1;
        if batch_i >= batch_size {
            flush_batch(FlushBatch {
                cfg,
                url,
                username,
                source: &source,
                name,
                header_line: &header_line,
                pending: &mut pending,
                pending_guids: &mut pending_guids,
                first_import,
                imported: &mut imported,
                journal,
                journal_path,
            })?;
            batch_i = 0;
        }
    }
    if !pending_guids.is_empty() {
        flush_batch(FlushBatch {
            cfg,
            url,
            username,
            source: &source,
            name,
            header_line: &header_line,
            pending: &mut pending,
            pending_guids: &mut pending_guids,
            first_import,
            imported: &mut imported,
            journal,
            journal_path,
        })?;
    }

    journal.files.insert(name.to_string());
    journal::append(
        journal_path,
        &JournalEvent::FileOk {
            url: url.to_string(),
            username: username.to_string(),
            source,
            file: name.to_string(),
        },
    )?;
    Ok((imported, attachment_count))
}

struct AssetUploadJob {
    digest: String,
    path: PathBuf,
    mime: Option<String>,
}

struct AssetUploadResult {
    digest: String,
    response: http::AssetPutResponse,
}

struct UploadAssets<'a> {
    input: &'a Path,
    name: &'a str,
    cfg: &'a VaultPushConfig,
    url: &'a str,
    username: &'a str,
    source: &'a str,
    unique: &'a BTreeMap<String, (String, Option<String>)>,
    journal: &'a mut JournalState,
    journal_path: &'a Path,
    assets_uploaded: &'a mut u64,
    assets_skipped: &'a mut u64,
    log: &'a mut LogWriter,
}

fn upload_assets(args: UploadAssets<'_>) -> Result<()> {
    let UploadAssets {
        input,
        name,
        cfg,
        url,
        username,
        source,
        unique,
        journal,
        journal_path,
        assets_uploaded,
        assets_skipped,
        log,
    } = args;
    let mut jobs = Vec::with_capacity(unique.len());
    for (digest, (rel, mime)) in unique {
        check_cancel(cfg.cancel.as_ref()).map_err(|_| anyhow::anyhow!("cancelled"))?;
        if !cfg.force && journal.assets.contains(digest) {
            *assets_skipped += 1;
            continue;
        }
        let path = resolve_attachment(input, rel)
            .ok_or_else(|| anyhow::anyhow!("{name}: missing attachment {rel}"))?;
        jobs.push(AssetUploadJob {
            digest: digest.clone(),
            path,
            mime: mime.clone(),
        });
    }
    if jobs.is_empty() {
        return Ok(());
    }

    let worker_count = cfg.asset_upload_workers.max(1).min(jobs.len());
    let next_job = AtomicUsize::new(0);
    let results = Mutex::new(
        std::iter::repeat_with(|| None)
            .take(jobs.len())
            .collect::<Vec<Option<Result<AssetUploadResult, String>>>>(),
    );
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let index = next_job.fetch_add(1, Ordering::Relaxed);
                    if index >= jobs.len() {
                        break;
                    }
                    let job = &jobs[index];
                    let result = check_cancel(cfg.cancel.as_ref())
                        .map_err(|_| "cancelled".to_string())
                        .and_then(|_| {
                            http::with_retries(cfg.max_retries, || {
                                http::put_asset(
                                    url,
                                    &cfg.key,
                                    username,
                                    source,
                                    &job.digest,
                                    &job.path,
                                    job.mime.as_deref(),
                                )
                            })
                            .map(|response| AssetUploadResult {
                                digest: job.digest.clone(),
                                response,
                            })
                            .map_err(|error| error.to_string())
                        });
                    results.lock().expect("asset result mutex poisoned")[index] = Some(result);
                }
            });
        }
    });

    let mut results = results.into_inner().expect("asset result mutex poisoned");
    for result in results.drain(..) {
        let result = result.expect("every asset job has a result");
        let uploaded = result.map_err(|error| anyhow::anyhow!("{name}: {error}"))?;
        journal.assets.insert(uploaded.digest.clone());
        journal::append(
            journal_path,
            &JournalEvent::AssetOk {
                url: url.to_string(),
                username: username.to_string(),
                source: source.to_string(),
                sha256: uploaded.digest.clone(),
            },
        )?;
        if uploaded.response.already_present {
            *assets_skipped += 1;
        } else {
            *assets_uploaded += 1;
        }
        log.line(&format!(
            "asset {} {}",
            if uploaded.response.already_present {
                "skip"
            } else {
                "ok"
            },
            uploaded.digest
        ));
    }
    Ok(())
}

struct FlushBatch<'a> {
    cfg: &'a VaultPushConfig,
    url: &'a str,
    username: &'a str,
    source: &'a str,
    name: &'a str,
    header_line: &'a [u8],
    pending: &'a mut Vec<u8>,
    pending_guids: &'a mut Vec<String>,
    first_import: &'a mut bool,
    imported: &'a mut u64,
    journal: &'a mut JournalState,
    journal_path: &'a Path,
}

fn flush_batch(args: FlushBatch<'_>) -> Result<()> {
    let mode = if args.cfg.mode == "replace" && *args.first_import {
        "replace"
    } else {
        "append"
    };
    let ndjson = std::mem::take(args.pending);
    let resp = http::with_retries(args.cfg.max_retries, || {
        http::post_import(
            args.url,
            &args.cfg.key,
            args.username,
            args.source,
            mode,
            ndjson.clone(),
        )
    })?;
    *args.first_import = false;
    *args.imported += resp.messages.max(resp.messages_appended);
    for guid in args.pending_guids.drain(..) {
        args.journal
            .messages
            .insert(JournalState::message_key(args.name, &guid));
        journal::append(
            args.journal_path,
            &JournalEvent::MessageOk {
                url: args.url.to_string(),
                username: args.username.to_string(),
                source: args.source.to_string(),
                file: args.name.to_string(),
                guid,
            },
        )?;
    }
    *args.pending = args.header_line.to_vec();
    Ok(())
}
