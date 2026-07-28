//! Full export pipeline (wtsexporter/JSON convert + media + obfuscate) for CLI and GUI.

use crate::cancel::{check_cancel, CancelFlag};
use crate::emit::{convert_json, ExportReport};
use crate::wtsexporter::{resolve_wtsexporter, run_wtsexporter, Platform, WtsexporterArgs};
use anyhow::{bail, Context, Result};
use message_csv::DateRange;
use message_media::{process_export_media, CompressOptions, MediaMode, MediaReport};
use message_obfuscate::{obfuscate_export_dir, resolve_obfuscator};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Inputs for a full WhatsApp export run.
#[derive(Debug, Clone)]
pub struct ExportConfig {
    /// Directory (or msgstore.db) used to resolve relative defaults. Defaults to cwd when unset.
    pub input: Option<PathBuf>,
    pub output: PathBuf,
    /// Required unless [`Self::json`] is set.
    pub platform: Option<Platform>,
    /// Skip wtsexporter; convert an existing result.json.
    pub json: Option<PathBuf>,
    pub key: Option<String>,
    pub backup: Option<PathBuf>,
    pub wa: Option<PathBuf>,
    pub media: Option<PathBuf>,
    pub db: Option<PathBuf>,
    pub business: bool,
    pub date_range: DateRange,
    pub media_mode: MediaMode,
    pub compress: CompressOptions,
    pub obfuscate: bool,
    pub obfuscate_seed: Option<String>,
    /// Cooperative cancel. Checked before/after wtsexporter and between chats in convert.
    /// Mid-run kill of the external wtsexporter process is not implemented.
    pub cancel: Option<CancelFlag>,
}

/// Result of [`run`]: convert report plus human-readable log lines.
#[derive(Debug)]
pub struct RunResult {
    pub report: ExportReport,
    pub messages: Vec<String>,
}

/// Resolve JSON (via wtsexporter or `--json`), convert, optionally process media and obfuscate.
pub fn run(config: &ExportConfig) -> Result<RunResult> {
    check_cancel(config.cancel.as_ref())?;
    let mut messages = Vec::new();

    let (json_path, media_roots, _work_keep_alive) = if let Some(json) = &config.json {
        let mut media_roots = Vec::new();
        if let Ok(cwd) = env::current_dir() {
            media_roots.push(cwd);
        }
        if let Some(input) = &config.input {
            media_roots.push(input.clone());
        }
        if let Some(parent) = json.parent() {
            media_roots.push(parent.to_path_buf());
        }
        media_roots.sort();
        media_roots.dedup();
        (json.clone(), media_roots, None)
    } else {
        let platform = config
            .platform
            .ok_or_else(|| anyhow::anyhow!("platform is required unless json is set"))?;
        let input = match config.input.clone() {
            Some(path) => path,
            None => env::current_dir().context("resolve current working directory")?,
        };

        check_cancel(config.cancel.as_ref())?;
        let bin = resolve_wtsexporter()?;
        fs::create_dir_all(&config.output)
            .with_context(|| format!("create {}", config.output.display()))?;
        // Scratch dir for wtsexporter cwd (iOS/Android extract) + result.json.
        // Kept until after convert so media copy can read extracted files.
        let work = tempfile::Builder::new()
            .prefix("wtsexporter-")
            .tempdir_in(&config.output)
            .context("create temp dir for wtsexporter")?;
        let json_out = work.path().join("result.json");
        let move_media = config.media_mode.copies_attachments() && config.media.is_some();

        // Cooperative only: we check cancel before and after the external process.
        // Killing wtsexporter mid-run is not implemented.
        check_cancel(config.cancel.as_ref())?;
        let log = run_wtsexporter(
            &bin,
            &WtsexporterArgs {
                platform,
                input: input.clone(),
                work_dir: work.path().to_path_buf(),
                key: config.key.clone(),
                backup: config.backup.clone(),
                wa: config.wa.clone(),
                media: config.media.clone(),
                db: config.db.clone(),
                business: config.business,
                move_media,
            },
            &json_out,
        )?;
        check_cancel(config.cancel.as_ref())?;

        if !log.trim().is_empty() {
            let trimmed = log.trim_end_matches('\n');
            messages.push(trimmed.to_string());
        }

        let kept = config.output.join("wtsexporter_result.json");
        fs::copy(&json_out, &kept).with_context(|| format!("copy JSON to {}", kept.display()))?;

        let mut media_roots = vec![work.path().to_path_buf(), input];
        if let Ok(cwd) = env::current_dir() {
            media_roots.push(cwd);
        }
        media_roots.sort();
        media_roots.dedup();

        (kept, media_roots, Some(work))
    };

    if !json_path.is_file() {
        bail!("JSON not found: {}", json_path.display());
    }

    check_cancel(config.cancel.as_ref())?;
    let report = convert_json(
        &json_path,
        &config.output,
        &config.date_range,
        config.media_mode.copies_attachments(),
        &media_roots,
        config.cancel.as_ref(),
    )?;
    // Drop tempdir after convert (media files already copied).
    drop(_work_keep_alive);

    if config.media_mode.needs_tools() {
        check_cancel(config.cancel.as_ref())?;
        let media = process_export_media(&config.output, config.media_mode, &config.compress)?;
        messages.extend(media_report_lines(&media));
        if !media.errors.is_empty() && media.processed == 0 {
            bail!("media processing failed for all candidate files");
        }
    }

    if config.obfuscate || config.obfuscate_seed.is_some() {
        check_cancel(config.cancel.as_ref())?;
        let mut anon = resolve_obfuscator(config.obfuscate_seed.as_deref())?;
        let n = obfuscate_export_dir(&config.output, &mut anon)?;
        messages.push(format!(
            "Obfuscated {n} CSV file(s) under {}",
            config.output.display()
        ));
    }

    messages.extend(report_summary_lines(&report, &config.output));
    Ok(RunResult { report, messages })
}

fn media_report_lines(report: &MediaReport) -> Vec<String> {
    if report.processed == 0 && report.skipped == 0 && report.errors.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![format!(
        "Media: processed {} file(s), skipped {}, updated {} CSV(s)",
        report.processed, report.skipped, report.csv_files_updated
    )];
    for err in report.errors.iter().take(10) {
        lines.push(format!("  media warning: {err}"));
    }
    if report.errors.len() > 10 {
        lines.push(format!("  …and {} more", report.errors.len() - 10));
    }
    lines
}

/// Format the convert summary the same way the CLI prints it.
pub fn report_summary_lines(report: &ExportReport, output: &Path) -> Vec<String> {
    let mut lines = vec![
        format!("Wrote {}", output.display()),
        format!("  conversations:      {}", report.conversations),
        format!("  messages:           {}", report.messages),
        format!("  attachments:        {}", report.attachments_saved),
    ];
    if report.attachments_missing > 0 {
        lines.push(format!(
            "  attachments missing:{}",
            report.attachments_missing
        ));
    }
    lines.push(format!(
        "  sent / received:    {} / {}",
        report.sent, report.received
    ));
    if report.skipped_invalid_date > 0 {
        lines.push(format!(
            "  skipped bad date:   {}",
            report.skipped_invalid_date
        ));
    }
    if report.skipped_out_of_range > 0 {
        lines.push(format!(
            "  skipped date range: {}",
            report.skipped_out_of_range
        ));
    }
    if !report.errors.is_empty() {
        lines.push(format!("  errors:             {}", report.errors.len()));
        for err in report.errors.iter().take(10) {
            lines.push(format!("    {err}"));
        }
    }
    lines
}

/// Helper used by CLI to parse date strings into [`DateRange`].
pub fn parse_date_range(
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Result<DateRange> {
    DateRange::parse(start_date, end_date)
        .map_err(anyhow::Error::msg)
        .context("invalid date range")
}
