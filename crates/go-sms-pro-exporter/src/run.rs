//! Full export pipeline (convert + media + obfuscate) for CLI and in-process GUI.

use crate::cancel::CancelFlag;
use crate::emit::{convert_export, ExportReport};
use anyhow::{Context, Result};
use message_contacts::resolve_contacts_cli;
use message_csv::DateRange;
use message_media::{process_export_media, CompressOptions, MediaMode, MediaReport};
use message_obfuscate::{obfuscate_export_dir, resolve_obfuscator};
use std::path::{Path, PathBuf};

/// Inputs for a full GO SMS Pro export run.
#[derive(Debug, Clone)]
pub struct ExportConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    pub owner_phones: Vec<String>,
    pub contacts: Option<PathBuf>,
    pub vcf: Option<PathBuf>,
    pub date_range: DateRange,
    pub media_mode: MediaMode,
    pub compress: CompressOptions,
    pub obfuscate: bool,
    pub obfuscate_seed: Option<String>,
    /// When set, convert checks this between XML/PDU files.
    pub cancel: Option<CancelFlag>,
}

/// Result of [`run`]: convert report plus human-readable log lines.
#[derive(Debug)]
pub struct RunResult {
    pub report: ExportReport,
    pub messages: Vec<String>,
}

/// Resolve contacts, convert, optionally process media and obfuscate.
pub fn run(config: &ExportConfig) -> Result<RunResult> {
    let mut messages = Vec::new();
    let (contacts, _) = resolve_contacts_cli(config.contacts.clone(), config.vcf.clone())?;
    let report = convert_export(
        &config.input,
        &config.output,
        &config.owner_phones,
        &contacts,
        &config.date_range,
        config.media_mode.copies_attachments(),
        config.cancel.as_ref(),
    )?;

    if config.media_mode.needs_tools() {
        let media = process_export_media(&config.output, config.media_mode, &config.compress)?;
        messages.extend(media_report_lines(&media));
        if !media.errors.is_empty() && media.processed == 0 {
            anyhow::bail!("media processing failed for all candidate files");
        }
    }

    if config.obfuscate || config.obfuscate_seed.is_some() {
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
        format!("  conversations:     {}", report.conversations),
        format!("  XML messages seen: {}", report.xml_messages_seen),
        format!("  PDU messages:      {}", report.pdu_messages),
        format!("  PDU group MMS:     {}", report.pdu_group_messages),
        format!("  attachments:       {}", report.attachments_saved),
        format!(
            "  sent / received:   {} / {}",
            report.sent, report.received
        ),
    ];
    if report.skipped_invalid_date > 0 {
        lines.push(format!(
            "  skipped bad date:  {}",
            report.skipped_invalid_date
        ));
    }
    if report.skipped_out_of_range > 0 {
        lines.push(format!(
            "  skipped date range:{}",
            report.skipped_out_of_range
        ));
    }
    if report.skipped_unknown_type > 0 {
        lines.push(format!(
            "  skipped bad type:  {}",
            report.skipped_unknown_type
        ));
    }
    if report.skipped_unknown_address > 0 {
        lines.push(format!(
            "  skipped invalid address: {}",
            report.skipped_unknown_address
        ));
        lines.push(format!(
            "  invalid-address detail: {}/skipped_invalid_address.csv",
            output.display()
        ));
        for d in report.skipped_unknown_address_details.iter().take(10) {
            lines.push(format!(
                "    invalid address: {} address={:?} contact={:?} type={} date_ms={} body={:?}",
                d.xml_file,
                d.address,
                d.contact_name,
                d.android_type,
                d.date_ms,
                d.body,
            ));
        }
        if report.skipped_unknown_address_details.len() > 10 {
            lines.push(format!(
                "    … and {} more (see skipped_invalid_address.csv)",
                report.skipped_unknown_address_details.len() - 10
            ));
        }
    }
    if report.skipped_empty_pdu > 0 {
        lines.push(format!("  skipped empty pdu: {}", report.skipped_empty_pdu));
        lines.push(format!(
            "  empty-pdu detail:   {}/skipped_empty_pdu.csv",
            output.display()
        ));
    }
    if report.skipped_no_other_party > 0 {
        lines.push(format!(
            "  skipped no party:  {}",
            report.skipped_no_other_party
        ));
        lines.push(format!(
            "  no-party detail:    {}/skipped_no_party.csv",
            output.display()
        ));
        for d in report.skipped_no_other_party_details.iter().take(10) {
            lines.push(format!(
                "    no party: {} participants=[{}] sent={} from={} to={}",
                d.pdu_filename,
                d.participants,
                d.is_sent as u8,
                d.has_from as u8,
                d.has_to as u8,
            ));
        }
        if report.skipped_no_other_party_details.len() > 10 {
            lines.push(format!(
                "    … and {} more (see skipped_no_party.csv)",
                report.skipped_no_other_party_details.len() - 10
            ));
        }
    }
    if report.skipped_unparseable_pdu > 0 {
        lines.push(format!(
            "  skipped bad PDU:   {}",
            report.skipped_unparseable_pdu
        ));
    }
    if !report.errors.is_empty() {
        lines.push(format!("  errors:            {}", report.errors.len()));
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
