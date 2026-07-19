use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Parser;
use message_contacts::ContactsBook;
use imazing_out::convert_export;

#[derive(Parser, Debug)]
#[command(name = "imazing-out")]
#[command(about = "Convert iMazing Messages CSV (+ Contacts CSV) to per-conversation CSV")]
struct Cli {
    /// iMazing Messages CSV file or directory of Messages CSVs
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
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
    )?;

    println!("Wrote {}", cli.output.display());
    match cli.contacts.as_ref() {
        Some(path) => println!("  contacts from:       {}", path.display()),
        None => println!("  contacts from:       (none)"),
    }
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
