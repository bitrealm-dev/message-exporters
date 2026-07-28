//! Full export pipeline (convert + obfuscate) for CLI and in-process GUI.

use crate::cancel::CancelFlag;
use crate::emit::{convert_export, ExportReport};
use anyhow::{Context, Result};
use message_contacts::resolve_contacts_cli;
use message_csv::DateRange;
use message_obfuscate::{obfuscate_export_dir, resolve_obfuscator};
use std::path::{Path, PathBuf};

/// Inputs for a full OpenExtract export run.
#[derive(Debug, Clone)]
pub struct ExportConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    pub contacts: Option<PathBuf>,
    pub vcf: Option<PathBuf>,
    pub date_range: DateRange,
    pub obfuscate: bool,
    pub obfuscate_seed: Option<String>,
    /// When set, convert checks this between CSV files.
    pub cancel: Option<CancelFlag>,
}

/// Result of [`run`]: convert report plus human-readable log lines.
#[derive(Debug)]
pub struct RunResult {
    pub report: ExportReport,
    pub messages: Vec<String>,
}

/// Resolve contacts, convert, optionally obfuscate.
pub fn run(config: &ExportConfig) -> Result<RunResult> {
    let mut messages = Vec::new();
    let (book, book_path) = resolve_contacts_cli(config.contacts.clone(), config.vcf.clone())?;
    let report = convert_export(
        &config.input,
        &config.output,
        &book,
        &config.date_range,
        config.cancel.as_ref(),
    )?;

    if config.obfuscate || config.obfuscate_seed.is_some() {
        let mut anon = resolve_obfuscator(config.obfuscate_seed.as_deref())?;
        let n = obfuscate_export_dir(&config.output, &mut anon)?;
        messages.push(format!(
            "Obfuscated {n} CSV file(s) under {}",
            config.output.display()
        ));
    }

    messages.extend(report_summary_lines(
        &report,
        &config.output,
        book_path.as_deref(),
    ));
    Ok(RunResult { report, messages })
}

/// Format the convert summary the same way the CLI prints it.
pub fn report_summary_lines(
    report: &ExportReport,
    output: &Path,
    contacts_path: Option<&Path>,
) -> Vec<String> {
    let mut lines = vec![
        format!("Wrote {}", output.display()),
        match contacts_path {
            Some(path) => format!("  contacts from:       {}", path.display()),
            None => "  contacts from:       (none)".to_string(),
        },
        format!("  conversations:       {}", report.conversations),
        format!("  messages:            {}", report.messages),
        format!(
            "  sent / received:     {} / {}",
            report.sent, report.received
        ),
    ];
    if report.skipped_invalid_date > 0 {
        lines.push(format!(
            "  skipped bad date:    {}",
            report.skipped_invalid_date
        ));
    }
    if report.skipped_out_of_range > 0 {
        lines.push(format!(
            "  skipped date range:  {}",
            report.skipped_out_of_range
        ));
    }
    if report.unresolved_chat_phone > 0 {
        lines.push(format!(
            "  unresolved phone:    {} (name-only chat ids; vault import may struggle)",
            report.unresolved_chat_phone
        ));
    }
    if !report.errors.is_empty() {
        lines.push(format!("  errors:              {}", report.errors.len()));
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
