use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use openextract_exporter::{parse_date_range, run, ExportConfig};

#[derive(Parser, Debug)]
#[command(name = "openextract-exporter")]
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

    /// Optional 8-hex seed for reproducible obfuscation (implies --obfuscate)
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
    let date_range = parse_date_range(cli.start_date.as_deref(), cli.end_date.as_deref())?;
    let result = run(&ExportConfig {
        input: cli.input,
        output: cli.output,
        contacts: cli.contacts,
        vcf: cli.vcf,
        date_range,
        obfuscate: cli.obfuscate,
        obfuscate_seed: cli.obfuscate_seed,
        cancel: None,
    })?;

    for line in &result.messages {
        if line.starts_with("Obfuscated ") {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
    Ok(())
}
