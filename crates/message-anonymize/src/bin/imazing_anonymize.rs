use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use message_anonymize::{anonymize_imazing, resolve_anonymizer};

#[derive(Parser, Debug)]
#[command(name = "imazing-anonymize")]
#[command(about = "Rewrite iMazing Messages CSV with anonymized names, numbers, text, and attachments")]
struct Cli {
    /// iMazing CSV file or directory of CSVs
    #[arg(long)]
    input: PathBuf,

    /// Output directory for anonymized CSV + placeholder attachments/
    #[arg(long)]
    output: PathBuf,

    /// Optional 64-char hex seed for reproducible (but non-reversible) remaps
    #[arg(long = "anonymize-seed")]
    anonymize_seed: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut anon = resolve_anonymizer(cli.anonymize_seed.as_deref())?;
    let n = anonymize_imazing(&cli.input, &cli.output, &mut anon)?;
    println!("Wrote {} anonymized CSV file(s) to {}", n, cli.output.display());
    Ok(())
}
