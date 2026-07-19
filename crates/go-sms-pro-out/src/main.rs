use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use go_sms_pro_out::convert_export;
use message_anonymize::{anonymize_near_vault_dir, resolve_anonymizer};
use message_contacts::resolve_contacts_cli;
use message_csv::DateRange;
use message_media::{
    compress_options_from_cli, eprint_report, process_near_vault_media, MaxResolution, MediaMode,
};

#[derive(Parser, Debug)]
#[command(name = "go-sms-pro-out")]
#[command(about = "Convert GO SMS Pro XML+PDU backups to per-conversation CSV")]
struct Cli {
    /// Directory containing gosms_sys*.xml and I_*.pdu files
    #[arg(long)]
    input: PathBuf,

    /// Output directory for CSV + attachments/
    #[arg(long)]
    output: PathBuf,

    /// Owner phone (E.164 or digits). Repeat for multiple owner numbers.
    /// Required — there is no demo default (wrong owner flips PDU direction).
    #[arg(long = "owner-phone", required = true)]
    owner_phones: Vec<String>,

    /// Vault-shaped contacts CSV (phones,first_name,last_name,…) for phone→name fill.
    /// Optional; without it (or `--vcf`) phone numbers are not resolved to names.
    #[arg(long)]
    contacts: Option<PathBuf>,

    /// Contacts VCF (alternate to `--contacts`).
    #[arg(long)]
    vcf: Option<PathBuf>,

    /// Rewrite output with stable, non-reversible fake names/numbers/text and placeholder media
    #[arg(long)]
    anonymize: bool,

    /// Optional 64-char hex seed for reproducible anonymization (implies --anonymize)
    #[arg(long = "anonymize-seed")]
    anonymize_seed: Option<String>,

    /// Only messages on or after this date (YYYY-MM-DD, local midnight, inclusive)
    #[arg(long = "start-date", value_name = "YYYY-MM-DD")]
    start_date: Option<String>,

    /// Only messages before this date (YYYY-MM-DD, local midnight, exclusive)
    #[arg(long = "end-date", value_name = "YYYY-MM-DD")]
    end_date: Option<String>,

    /// Attachment media: disabled (no files), clone (default), convert, or compress
    #[arg(long = "media-mode", default_value = "clone", value_name = "MODE")]
    media_mode: MediaMode,

    /// Compress only: max long edge (720p, 1080p, 4k)
    #[arg(long = "media-max-resolution", default_value = "1080p", value_name = "RES")]
    media_max_resolution: MaxResolution,

    /// Compress only: max frame rate
    #[arg(long = "media-max-fps", default_value_t = 30.0)]
    media_max_fps: f32,

    /// Compress only: only re-encode videos at/above this size (e.g. 20M)
    #[arg(long = "media-min-size", default_value = "20M")]
    media_min_size: String,

    /// Compress only: skip already-efficient HEVC under max resolution (default on)
    #[arg(long = "media-skip-efficient", default_value_t = true, action = clap::ArgAction::Set)]
    media_skip_efficient: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let date_range = DateRange::parse(cli.start_date.as_deref(), cli.end_date.as_deref())
        .map_err(anyhow::Error::msg)
        .context("invalid date range")?;
    let (contacts, _) = resolve_contacts_cli(cli.contacts, cli.vcf)?;
    let report = convert_export(
        &cli.input,
        &cli.output,
        &cli.owner_phones,
        &contacts,
        &date_range,
        cli.media_mode.copies_attachments(),
    )?;

    if cli.media_mode.needs_tools() {
        let compress = compress_options_from_cli(
            cli.media_max_resolution,
            cli.media_max_fps,
            &cli.media_min_size,
            cli.media_skip_efficient,
        )?;
        let media = process_near_vault_media(&cli.output, cli.media_mode, &compress)?;
        eprint_report(&media);
        if !media.errors.is_empty() && media.processed == 0 {
            anyhow::bail!("media processing failed for all candidate files");
        }
    }

    if cli.anonymize || cli.anonymize_seed.is_some() {
        let mut anon = resolve_anonymizer(cli.anonymize_seed.as_deref())?;
        let n = anonymize_near_vault_dir(&cli.output, &mut anon)?;
        eprintln!("Anonymized {n} CSV file(s) under {}", cli.output.display());
    }

    println!("Wrote {}", cli.output.display());
    println!("  conversations:     {}", report.conversations);
    println!("  XML messages seen: {}", report.xml_messages_seen);
    println!("  PDU messages:      {}", report.pdu_messages);
    println!("  PDU group MMS:     {}", report.pdu_group_messages);
    println!("  attachments:       {}", report.attachments_saved);
    println!("  sent / received:   {} / {}", report.sent, report.received);
    if report.skipped_invalid_date > 0 {
        println!("  skipped bad date:  {}", report.skipped_invalid_date);
    }
    if report.skipped_out_of_range > 0 {
        println!("  skipped date range:{}", report.skipped_out_of_range);
    }
    if report.skipped_unknown_type > 0 {
        println!("  skipped bad type:  {}", report.skipped_unknown_type);
    }
    if report.skipped_unknown_address > 0 {
        println!("  skipped bad addr:  {}", report.skipped_unknown_address);
    }
    if report.skipped_no_other_party > 0 {
        println!("  skipped no party:  {}", report.skipped_no_other_party);
    }
    if report.skipped_unparseable_pdu > 0 {
        println!("  skipped bad PDU:   {}", report.skipped_unparseable_pdu);
    }
    if !report.errors.is_empty() {
        println!("  errors:            {}", report.errors.len());
        for err in report.errors.iter().take(10) {
            println!("    {err}");
        }
    }
    Ok(())
}
