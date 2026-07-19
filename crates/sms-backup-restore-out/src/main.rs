use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use message_anonymize::{anonymize_near_vault_dir, resolve_anonymizer};
use message_contacts::resolve_contacts_cli;
use sms_backup_restore_out::convert_export;

#[derive(Parser, Debug)]
#[command(name = "sms-backup-restore-out")]
#[command(about = "Convert SMS Backup & Restore XML to per-conversation CSV")]
struct Cli {
    /// Path to sms-*.xml file, or a directory of .xml files
    #[arg(long)]
    input: PathBuf,

    /// Output directory for CSV + attachments/
    #[arg(long)]
    output: PathBuf,

    /// Owner phone (E.164 or digits). Repeat for multiple owner numbers.
    /// Required — there is no demo default (wrong owner flips MMS chat keys).
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let (contacts, _) = resolve_contacts_cli(cli.contacts, cli.vcf)?;
    let report = convert_export(&cli.input, &cli.output, &cli.owner_phones, &contacts)?;

    if cli.anonymize || cli.anonymize_seed.is_some() {
        let mut anon = resolve_anonymizer(cli.anonymize_seed.as_deref())?;
        let n = anonymize_near_vault_dir(&cli.output, &mut anon)?;
        eprintln!("Anonymized {n} CSV file(s) under {}", cli.output.display());
    }

    println!("Wrote {}", cli.output.display());
    println!("  conversations:     {}", report.conversations);
    println!("  SMS / MMS seen:    {} / {}", report.sms_seen, report.mms_seen);
    println!("  attachments:       {}", report.attachments_saved);
    println!("  sent / received:   {} / {}", report.sent, report.received);
    if report.skipped_invalid_date > 0 {
        println!("  skipped bad date:  {}", report.skipped_invalid_date);
    }
    if report.skipped_unknown_type > 0 {
        println!("  skipped bad type:  {}", report.skipped_unknown_type);
    }
    if report.skipped_draft_or_outbox > 0 {
        println!("  skipped draft/out: {}", report.skipped_draft_or_outbox);
    }
    if report.skipped_unknown_address > 0 {
        println!("  skipped bad addr:  {}", report.skipped_unknown_address);
    }
    if report.skipped_empty_participants > 0 {
        println!("  skipped empty:     {}", report.skipped_empty_participants);
    }
    if report.skipped_bad_attachment > 0 {
        println!("  skipped bad att:   {}", report.skipped_bad_attachment);
    }
    if !report.errors.is_empty() {
        println!("  errors:            {}", report.errors.len());
        for err in report.errors.iter().take(10) {
            println!("    {err}");
        }
    }
    Ok(())
}
