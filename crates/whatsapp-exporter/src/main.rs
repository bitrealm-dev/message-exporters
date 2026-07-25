use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use message_csv::DateRange;
use message_media::{
    compress_options_from_cli, eprint_report, process_export_media, MaxResolution, MediaMode,
};
use message_obfuscate::{obfuscate_export_dir, resolve_obfuscator};
use whatsapp_exporter::{
    convert_json, resolve_wtsexporter, run_wtsexporter, Platform, WtsexporterArgs,
};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliPlatform {
    Android,
    Ios,
}

impl From<CliPlatform> for Platform {
    fn from(value: CliPlatform) -> Self {
        match value {
            CliPlatform::Android => Platform::Android,
            CliPlatform::Ios => Platform::Ios,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "whatsapp-exporter")]
#[command(about = "Convert WhatsApp DB/backup (via wtsexporter) to per-conversation CSV")]
struct Cli {
    /// Optional working directory for wtsexporter relative defaults (default: process cwd).
    /// Not used by the GUI — it relies on the GUI launch directory.
    #[arg(long)]
    input: Option<PathBuf>,

    /// Output directory for per-conversation CSV (+ attachments/)
    #[arg(long)]
    output: PathBuf,

    /// Android or iOS (required unless --json)
    #[arg(long, value_enum)]
    platform: Option<CliPlatform>,

    /// Skip wtsexporter; convert an existing result.json
    #[arg(long)]
    json: Option<PathBuf>,

    /// Decryption key file path or crypt15 hex key (forwarded as -k)
    #[arg(long)]
    key: Option<String>,

    /// Encrypted backup / iOS backup path (forwarded as -b)
    #[arg(long)]
    backup: Option<PathBuf>,

    /// Contacts database wa.db / ContactsV2.sqlite (forwarded as -w)
    #[arg(long)]
    wa: Option<PathBuf>,

    /// WhatsApp media folder (forwarded as -m)
    #[arg(long)]
    media: Option<PathBuf>,

    /// Explicit msgstore.db path (forwarded as -d)
    #[arg(long)]
    db: Option<PathBuf>,

    /// WhatsApp Business defaults
    #[arg(long)]
    business: bool,

    /// Rewrite output with stable fake names/numbers/text and placeholder media
    #[arg(long)]
    obfuscate: bool,

    /// Optional 8-hex seed for reproducible obfuscation (implies --obfuscate)
    #[arg(long = "obfuscate-seed")]
    obfuscate_seed: Option<String>,

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

    let json_path = if let Some(json) = &cli.json {
        json.clone()
    } else {
        let platform = cli
            .platform
            .ok_or_else(|| anyhow::anyhow!("--platform is required unless --json is set"))?;
        let input = match cli.input.clone() {
            Some(path) => path,
            None => env::current_dir().context("resolve current working directory")?,
        };

        let bin = resolve_wtsexporter()?;
        fs::create_dir_all(&cli.output)
            .with_context(|| format!("create {}", cli.output.display()))?;
        let work = tempfile::Builder::new()
            .prefix("wtsexporter-")
            .tempdir_in(&cli.output)
            .context("create temp dir for wtsexporter")?;
        let json_out = work.path().join("result.json");
        let move_media = cli.media_mode.copies_attachments() && cli.media.is_some();
        let log = run_wtsexporter(
            &bin,
            &WtsexporterArgs {
                platform: platform.into(),
                input,
                key: cli.key.clone(),
                backup: cli.backup.clone(),
                wa: cli.wa.clone(),
                media: cli.media.clone(),
                db: cli.db.clone(),
                business: cli.business,
                move_media,
            },
            &json_out,
        )?;
        if !log.trim().is_empty() {
            eprint!("{log}");
            if !log.ends_with('\n') {
                eprintln!();
            }
        }
        let kept = cli.output.join("wtsexporter_result.json");
        fs::copy(&json_out, &kept).with_context(|| format!("copy JSON to {}", kept.display()))?;
        drop(work);
        kept
    };

    if !json_path.is_file() {
        bail!("JSON not found: {}", json_path.display());
    }

    let mut media_roots = Vec::new();
    if let Ok(cwd) = env::current_dir() {
        media_roots.push(cwd);
    }
    if let Some(input) = &cli.input {
        media_roots.push(input.clone());
    }
    if let Some(parent) = json_path.parent() {
        media_roots.push(parent.to_path_buf());
    }
    media_roots.sort();
    media_roots.dedup();

    let report = convert_json(
        &json_path,
        &cli.output,
        &date_range,
        cli.media_mode.copies_attachments(),
        &media_roots,
    )?;

    if cli.media_mode.needs_tools() {
        let compress = compress_options_from_cli(
            cli.media_max_resolution,
            cli.media_max_fps,
            &cli.media_min_size,
            cli.media_skip_efficient,
        )?;
        let media = process_export_media(&cli.output, cli.media_mode, &compress)?;
        eprint_report(&media);
        if !media.errors.is_empty() && media.processed == 0 {
            bail!("media processing failed for all candidate files");
        }
    }

    if cli.obfuscate || cli.obfuscate_seed.is_some() {
        let mut anon = resolve_obfuscator(cli.obfuscate_seed.as_deref())?;
        let n = obfuscate_export_dir(&cli.output, &mut anon)?;
        eprintln!("Obfuscated {n} CSV file(s) under {}", cli.output.display());
    }

    println!("Wrote {}", cli.output.display());
    println!("  conversations:      {}", report.conversations);
    println!("  messages:           {}", report.messages);
    println!("  attachments:        {}", report.attachments_saved);
    if report.attachments_missing > 0 {
        println!("  attachments missing:{}", report.attachments_missing);
    }
    println!("  sent / received:    {} / {}", report.sent, report.received);
    if report.skipped_invalid_date > 0 {
        println!("  skipped bad date:   {}", report.skipped_invalid_date);
    }
    if report.skipped_out_of_range > 0 {
        println!("  skipped date range: {}", report.skipped_out_of_range);
    }
    if !report.errors.is_empty() {
        println!("  errors:             {}", report.errors.len());
        for err in report.errors.iter().take(10) {
            println!("    {err}");
        }
    }
    Ok(())
}
