use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use message_contacts::resolve_contacts_cli;
use openextract_to_csv::convert_export;

#[derive(Parser, Debug)]
#[command(name = "openextract-to-csv")]
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

    /// Vault-shaped contacts CSV (phones,first_name,last_name) instead of --vcf
    #[arg(long)]
    contacts: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let (book, book_path) = resolve_contacts_cli(cli.contacts, cli.vcf)?;
    let report = convert_export(&cli.input, &cli.output, &book)?;

    println!("Wrote {}", cli.output.display());
    println!("  contacts from:       {}", book_path.display());
    println!("  conversations:       {}", report.conversations);
    println!("  messages:            {}", report.messages);
    println!("  sent / received:     {} / {}", report.sent, report.received);
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
