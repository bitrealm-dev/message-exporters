use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;
use imazing_out::convert_export;
use message_anonymize::{anonymize_near_vault_dir, resolve_anonymizer};
use message_contacts::ContactsBook;
use message_csv::DateRange;

#[derive(Parser, Debug)]
#[command(name = "imazing-out")]
#[command(about = "Convert iMazing Messages / WhatsApp CSV exports to per-conversation CSV")]
struct Cli {
    /// One Messages/WhatsApp CSV, a chat folder, Messages/, WhatsApp/, or a full device export root
    #[arg(long)]
    input: PathBuf,

    /// Output directory for per-conversation CSV files
    #[arg(long)]
    output: PathBuf,

    /// iMazing Contacts CSV from the same backup export.
    /// Optional; without it phone numbers are not resolved to names.
    #[arg(long)]
    contacts: Option<PathBuf>,

    /// IANA timezone for naive Message Date values (default: host local)
    #[arg(long)]
    timezone: Option<String>,

    /// Only messages on or after this date (YYYY-MM-DD, timezone midnight, inclusive)
    #[arg(long = "start-date", value_name = "YYYY-MM-DD")]
    start_date: Option<String>,

    /// Only messages before this date (YYYY-MM-DD, timezone midnight, exclusive)
    #[arg(long = "end-date", value_name = "YYYY-MM-DD")]
    end_date: Option<String>,

    /// Rewrite output with stable, non-reversible fake names/numbers/text and placeholder media
    #[arg(long)]
    anonymize: bool,

    /// Optional 64-char hex seed for reproducible anonymization (implies --anonymize)
    #[arg(long = "anonymize-seed")]
    anonymize_seed: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let date_range = DateRange::parse_optional_tz(
        cli.start_date.as_deref(),
        cli.end_date.as_deref(),
        cli.timezone.as_deref(),
    )
    .map_err(anyhow::Error::msg)
    .context("invalid date range")?;
    let book = match cli.contacts.as_ref() {
        Some(path) => {
            if !path.is_file() {
                bail!("contacts file not found: {}", path.display());
            }
            ContactsBook::load_imazing_contacts_csv(path)?
        }
        None => {
            eprintln!(
                "warning: no contacts file provided (--contacts); \
                 phone numbers will not be resolved to names"
            );
            ContactsBook::empty()
        }
    };
    let report = convert_export(
        &cli.input,
        &cli.output,
        &book,
        cli.timezone.as_deref(),
        &date_range,
    )?;

    if cli.anonymize || cli.anonymize_seed.is_some() {
        let mut anon = resolve_anonymizer(cli.anonymize_seed.as_deref())?;
        let n = anonymize_near_vault_dir(&cli.output, &mut anon)?;
        eprintln!("Anonymized {n} CSV file(s) under {}", cli.output.display());
    }

    println!("Wrote {}", cli.output.display());
    match cli.contacts.as_ref() {
        Some(path) => println!("  contacts from:       {}", path.display()),
        None => println!("  contacts from:       (none)"),
    }
    println!("  messages CSVs:       {}", report.messages_files);
    println!("  whatsapp CSVs:       {}", report.whatsapp_files);
    println!("  conversations:       {}", report.conversations);
    println!("  messages:            {}", report.messages);
    println!("  sent / received:     {} / {}", report.sent, report.received);
    if report.notifications > 0 {
        println!("  notifications:       {}", report.notifications);
    }
    if report.duplicates_dropped > 0 {
        println!("  duplicates dropped:  {}", report.duplicates_dropped);
    }
    if report.skipped_invalid_date > 0 {
        println!("  skipped bad date:    {}", report.skipped_invalid_date);
    }
    if report.skipped_out_of_range > 0 {
        println!("  skipped date range:  {}", report.skipped_out_of_range);
    }
    if report.unresolved_chat_phone > 0 {
        println!(
            "  unresolved phone:    {} (name-only chat ids; vault import may struggle)",
            report.unresolved_chat_phone
        );
    }
    if report.unresolved_group_participants > 0 {
        println!(
            "  unresolved members:  {} (group roster names with no phone in contacts)",
            report.unresolved_group_participants
        );
    }
    if !report.errors.is_empty() {
        println!("  errors:              {}", report.errors.len());
        for err in report.errors.iter().take(10) {
            println!("    {err}");
        }
    }
    Ok(())
}
