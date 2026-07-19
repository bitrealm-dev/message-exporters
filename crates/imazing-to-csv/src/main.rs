use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Parser;
use message_contacts::ContactsBook;
use imazing_to_csv::convert_export;

#[derive(Parser, Debug)]
#[command(name = "imazing-to-csv")]
#[command(about = "Convert iMazing Messages CSV (+ Contacts CSV) to per-conversation CSV")]
struct Cli {
    /// iMazing Messages CSV file or directory of Messages CSVs
    #[arg(long)]
    input: PathBuf,

    /// Output directory for per-conversation CSV files
    #[arg(long)]
    output: PathBuf,

    /// iMazing Contacts CSV from the same backup export (required)
    #[arg(long)]
    contacts: PathBuf,

    /// IANA timezone for naive Message Date values (default: host local)
    #[arg(long)]
    timezone: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if !cli.contacts.is_file() {
        bail!("contacts file not found: {}", cli.contacts.display());
    }
    let book = ContactsBook::load_imazing_contacts_csv(&cli.contacts)?;
    let report = convert_export(
        &cli.input,
        &cli.output,
        &book,
        cli.timezone.as_deref(),
    )?;

    println!("Wrote {}", cli.output.display());
    println!("  contacts from:       {}", cli.contacts.display());
    println!("  conversations:       {}", report.conversations);
    println!("  messages:            {}", report.messages);
    println!("  sent / received:     {} / {}", report.sent, report.received);
    if report.duplicates_dropped > 0 {
        println!("  duplicates dropped:  {}", report.duplicates_dropped);
    }
    if report.skipped_invalid_date > 0 {
        println!("  skipped bad date:    {}", report.skipped_invalid_date);
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
