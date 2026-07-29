//! Full export pipeline for CLI and in-process GUI.

use crate::emit::{ExportReport, convert_export};
use anyhow::{Context, Result, bail};
use message_contacts::ContactsBook;
use message_csv::DateRange;
use message_exporters_core::{ExporterConfig, SourceConfig};
use message_ir::ExportTransforms;
use std::path::Path;

/// Result of [`run`]: convert report plus human-readable log lines.
#[derive(Debug)]
pub struct RunResult {
    pub report: ExportReport,
    pub messages: Vec<String>,
}

/// Load contacts, convert, apply media/obfuscate via FormatSink.
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let SourceConfig::Imazing(source) = &config.source else {
        bail!("imazing-exporter requires SourceConfig::Imazing");
    };
    let input = config.require_input().map_err(anyhow::Error::msg)?;
    let mut messages = Vec::new();
    let (contacts_csv, _) = config.contacts_csv_vcf();
    let book = match contacts_csv.as_ref() {
        Some(path) => {
            if !path.is_file() {
                bail!("contacts file not found: {}", path.display());
            }
            ContactsBook::load_imazing_contacts_csv(path)?
        }
        None => {
            messages.push(
                "warning: no contacts file provided (--contacts); \
                 phone numbers will not be resolved to names"
                    .to_string(),
            );
            ContactsBook::empty()
        }
    };

    let transforms = ExportTransforms::from_configs(&config.media, &config.obfuscate);
    let (report, sink) = convert_export(
        input,
        &config.output,
        &book,
        source.timezone.as_deref(),
        &config.date_range,
        transforms,
        config.output_format,
        config.cancel.as_ref(),
    )?;
    if !sink.media.errors.is_empty() && sink.media.processed == 0 && config.media.mode.needs_tools()
    {
        anyhow::bail!("media processing failed for all candidate files");
    }
    messages.extend(sink.log_lines());

    messages.extend(report_summary_lines(
        &report,
        &config.output,
        contacts_csv.as_deref(),
    ));
    Ok(RunResult { report, messages })
}

/// Format the convert summary the same way the CLI prints it.
pub fn report_summary_lines(
    report: &ExportReport,
    output: &Path,
    contacts: Option<&Path>,
) -> Vec<String> {
    let mut lines = vec![
        format!("Wrote {}", output.display()),
        match contacts {
            Some(path) => format!("  contacts from:       {}", path.display()),
            None => "  contacts from:       (none)".to_string(),
        },
        format!("  messages CSVs:       {}", report.messages_files),
        format!("  whatsapp CSVs:       {}", report.whatsapp_files),
        format!("  conversations:       {}", report.conversations),
        format!("  messages:            {}", report.messages),
        format!("  attachments:         {}", report.attachments_saved),
        format!(
            "  sent / received:     {} / {}",
            report.sent, report.received
        ),
    ];
    if report.notifications > 0 {
        lines.push(format!("  notifications:       {}", report.notifications));
    }
    if report.duplicates_dropped > 0 {
        lines.push(format!(
            "  duplicates dropped:  {}",
            report.duplicates_dropped
        ));
    }
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
    if report.unresolved_group_participants > 0 {
        lines.push(format!(
            "  unresolved members:  {} (group roster names with no phone in contacts)",
            report.unresolved_group_participants
        ));
    }
    if !report.errors.is_empty() {
        lines.push(format!("  errors:              {}", report.errors.len()));
        for err in report.errors.iter().take(10) {
            lines.push(format!("  - {err}"));
        }
    }
    lines
}

/// Helper used by CLI to parse date strings with optional timezone into [`DateRange`].
pub fn parse_date_range(
    start_date: Option<&str>,
    end_date: Option<&str>,
    timezone: Option<&str>,
) -> Result<DateRange> {
    DateRange::parse_optional_tz(start_date, end_date, timezone)
        .map_err(anyhow::Error::msg)
        .context("invalid date range")
}
