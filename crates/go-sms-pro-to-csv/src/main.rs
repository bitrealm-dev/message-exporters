use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use go_sms_pro_to_csv::convert_export;
use message_anonymize::{anonymize_near_vault_dir, resolve_anonymizer};
use message_contacts::resolve_contacts_cli;

#[derive(Parser, Debug)]
#[command(name = "go-sms-pro-to-csv")]
#[command(about = "Convert GO SMS Pro XML+PDU backups to per-conversation CSV")]
struct Cli {
    /// Directory containing gosms_sys*.xml and I_*.pdu files
    #[arg(long)]
    input: PathBuf,

    /// Output directory for CSV + attachments/
    #[arg(long)]
    output: PathBuf,

    /// Owner phone (E.164 or digits). Repeat for multiple owner numbers.
    /// Required — there is no demo default (wrong owner flips PDU direction).
    #[arg(long = "owner-phone", required = true)]
    owner_phones: Vec<String>,

    /// Vault-shaped contacts CSV (phones,first_name,last_name,…) for phone→name fill.
    /// Required unless `--vcf` is set.
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
    println!("  XML messages seen: {}", report.xml_messages_seen);
    println!("  PDU messages:      {}", report.pdu_messages);
    println!("  PDU group MMS:     {}", report.pdu_group_messages);
    println!("  attachments:       {}", report.attachments_saved);
    println!("  sent / received:   {} / {}", report.sent, report.received);
    if report.skipped_invalid_date > 0 {
        println!("  skipped bad date:  {}", report.skipped_invalid_date);
    }
    if report.skipped_unknown_type > 0 {
        println!("  skipped bad type:  {}", report.skipped_unknown_type);
    }
    if report.skipped_unknown_address > 0 {
        println!("  skipped bad addr:  {}", report.skipped_unknown_address);
    }
    if report.skipped_no_other_party > 0 {
        println!("  skipped no party:  {}", report.skipped_no_other_party);
    }
    if report.skipped_unparseable_pdu > 0 {
        println!("  skipped bad PDU:   {}", report.skipped_unparseable_pdu);
    }
    if !report.errors.is_empty() {
        println!("  errors:            {}", report.errors.len());
        for err in report.errors.iter().take(10) {
            println!("    {err}");
        }
    }
    Ok(())
}
