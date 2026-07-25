use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use message_obfuscate::{obfuscate_export_dir, resolve_obfuscator};
use message_contacts::resolve_contacts_cli;
use message_csv::DateRange;
use openextract_out::convert_export;

#[derive(Parser, Debug)]
#[command(name = "openextract-out")]
#[command(about = "Convert OpenExtract conversation CSV (+ VCF) to per-conversation CSV")]
struct Cli {
    /// OpenExtract CSV file or directory of conversation_*.csv / all_conversations.csv
    #[arg(long)]
    input: PathBuf,

    /// Output directory for per-conversation CSV files
    #[arg(long)]
    output: PathBuf,

    /// Contacts VCF from the OpenExtract export (phone ↔ name)
    #[arg(long)]
    vcf: Option<PathBuf>,

    /// Contacts file instead of --vcf (VCF or iMazing Contacts CSV; same as contacts-validate)
    #[arg(long)]
    contacts: Option<PathBuf>,

    /// Rewrite output with stable, non-reversible fake names/numbers/text and placeholder media
    #[arg(long)]
    obfuscate: bool,

    /// Optional 64-char hex seed for reproducible obfuscation (implies --obfuscate)
    #[arg(long = "obfuscate-seed")]
    obfuscate_seed: Option<String>,

    /// Only messages on or after this date (YYYY-MM-DD, local midnight, inclusive)
    #[arg(long = "start-date", value_name = "YYYY-MM-DD")]
    start_date: Option<String>,

    /// Only messages before this date (YYYY-MM-DD, local midnight, exclusive)
    #[arg(long = "end-date", value_name = "YYYY-MM-DD")]
    end_date: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let date_range = DateRange::parse(cli.start_date.as_deref(), cli.end_date.as_deref())
        .map_err(anyhow::Error::msg)
        .context("invalid date range")?;
    let (book, book_path) = resolve_contacts_cli(cli.contacts, cli.vcf)?;
    let report = convert_export(&cli.input, &cli.output, &book, &date_range)?;

    if cli.obfuscate || cli.obfuscate_seed.is_some() {
        let mut anon = resolve_obfuscator(cli.obfuscate_seed.as_deref())?;
        let n = obfuscate_export_dir(&cli.output, &mut anon)?;
        eprintln!("Obfuscated {n} CSV file(s) under {}", cli.output.display());
    }

    println!("Wrote {}", cli.output.display());
    match book_path.as_ref() {
        Some(path) => println!("  contacts from:       {}", path.display()),
        None => println!("  contacts from:       (none)"),
    }
    println!("  conversations:       {}", report.conversations);
    println!("  messages:            {}", report.messages);
    println!("  sent / received:     {} / {}", report.sent, report.received);
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
    if !report.errors.is_empty() {
        println!("  errors:              {}", report.errors.len());
        for err in report.errors.iter().take(10) {
            println!("    {err}");
        }
    }
    Ok(())
}
