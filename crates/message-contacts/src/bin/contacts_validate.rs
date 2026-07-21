//! Copy a contacts VCF/CSV and rewrite only unambiguous phone numbers.

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Parser;
use message_contacts::{validate_contacts_file, ValidateMode};
use message_phone::PhoneRegion;

#[derive(Parser, Debug)]
#[command(name = "contacts-validate")]
#[command(about = "Check or update contacts phones; write corrected copy on update")]
struct Cli {
    /// Contacts file (.vcf, vault CSV, or iMazing Contacts CSV)
    #[arg(long)]
    input: PathBuf,

    /// usa | international
    #[arg(long, default_value = "usa")]
    region: String,

    /// Analyze only; print the validate log to stdout and write nothing
    #[arg(long)]
    check: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let Some(region) = PhoneRegion::parse_cli(&cli.region) else {
        bail!("unknown --region {:?} (use usa or international)", cli.region);
    };

    let mode = if cli.check {
        ValidateMode::Check
    } else {
        ValidateMode::Update
    };
    let report = validate_contacts_file(&cli.input, region, mode)?;

    if cli.check {
        for line in &report.log_lines {
            println!("{line}");
        }
        println!();
        println!(
            "Check complete — rewritten={} uncertain={} duplicate_groups={} (no files written)",
            report.rewritten, report.uncertain, report.duplicate_groups
        );
    } else {
        println!("Wrote {}", report.output_path.display());
        if let Some(vcf) = &report.vcf_path {
            println!("  vcf:               {}", vcf.display());
        }
        println!("  log:               {}", report.log_path.display());
        println!("  rewritten phones:  {}", report.rewritten);
        println!("  uncertain phones:  {}", report.uncertain);
        println!("  duplicate groups:  {}", report.duplicate_groups);
    }
    Ok(())
}
