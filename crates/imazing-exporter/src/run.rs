//! Full export pipeline (convert + media + obfuscate) for CLI and in-process GUI.

use crate::emit::{convert_export, ExportReport};
use anyhow::{bail, Context, Result};
use message_contacts::ContactsBook;
use message_csv::DateRange;
use message_exporters_core::{ExporterConfig, OutputFormat, SourceConfig};
use message_media::{process_export_media, MediaReport};
use message_obfuscate::{obfuscate_export_dir, resolve_obfuscator};
use std::path::Path;

/// Result of [`run`]: convert report plus human-readable log lines.
#[derive(Debug)]
pub struct RunResult {
    pub report: ExportReport,
    pub messages: Vec<String>,
}

/// Load contacts, convert, optionally process media and obfuscate.
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

    let report = convert_export(
        input,
        &config.output,
        &book,
        source.timezone.as_deref(),
        &config.date_range,
        config.media.mode.copies_attachments(),
        config.output_format,
        config.cancel.as_ref(),
    )?;

    // Media convert/compress and CSV obfuscation apply to CSV output only.
    if config.output_format == OutputFormat::Csv {
        if config.media.mode.needs_tools() {
            let media =
                process_export_media(&config.output, config.media.mode, &config.media.compress)?;
            messages.extend(media_report_lines(&media));
            if !media.errors.is_empty() && media.processed == 0 {
                anyhow::bail!("media processing failed for all candidate files");
            }
        }

        if config.obfuscate_active() {
            let mut anon = resolve_obfuscator(config.obfuscate.seed.as_deref())?;
            let n = obfuscate_export_dir(&config.output, &mut anon)?;
            messages.push(format!(
                "Obfuscated {n} CSV file(s) under {}",
                config.output.display()
            ));
        }
    }

    messages.extend(report_summary_lines(
        &report,
        &config.output,
        contacts_csv.as_deref(),
    ));
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
        lines.push(format!(
            "  notifications:       {}",
            report.notifications
        ));
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
