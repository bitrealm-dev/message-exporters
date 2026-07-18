use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use go_sms_pro_to_csv::convert_export;

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let report = convert_export(&cli.input, &cli.output, &cli.owner_phones)?;

    println!("Wrote {}", cli.output.display());
    println!("  conversations:     {}", report.conversations);
    println!("  XML messages:      {}", report.xml_messages);
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
