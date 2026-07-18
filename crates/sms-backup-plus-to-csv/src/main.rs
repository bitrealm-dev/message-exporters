use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use sms_backup_plus_to_csv::convert_export;

#[derive(Parser, Debug)]
#[command(name = "sms-backup-plus-to-csv")]
#[command(about = "Convert SMS Backup+ EML exports to per-conversation CSV")]
struct Cli {
    /// Log progress to stderr (inputs, scan/write progress, dedupe summary)
    #[arg(short = 'v', long, global = true)]
    verbose: bool,

    /// Skip the end-of-run summary on stdout
    #[arg(long, global = true)]
    no_summary: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Convert EML tree to per-conversation CSV
    Convert {
        /// Path to a .eml file or directory tree of EMLs (Archive/, Sent/, …).
        /// Repeat for multiple roots; trees are merged and path-deduped.
        /// Default: source_dirs from config/owner.toml when set.
        #[arg(long = "input")]
        input: Vec<PathBuf>,

        /// Output directory for CSV + attachments/
        #[arg(long)]
        output: PathBuf,

        /// Owner phone (E.164 or digits). Repeat for multiple owner numbers.
        /// Default: config/owner.toml `phone`
        #[arg(long = "owner-phone")]
        owner_phones: Vec<String>,

        /// Owner email addresses used to detect sent messages when X-smssync-type is missing.
        /// Default: config/owner.toml
        #[arg(long = "owner-email", value_name = "EMAIL")]
        owner_emails: Vec<String>,

        /// Contacts CSV (phones,first_name,last_name,…) for name→phone lookup.
        /// Default: config/contacts.csv when that file exists.
        #[arg(long)]
        contacts: Option<PathBuf>,

        /// Name mapping CSV (correct_name,incorrect_name) for EML export aliases.
        /// Default: repo config/name-mapping.csv when that file exists.
        #[arg(long = "name-mapping")]
        name_mapping: Option<PathBuf>,
    },
}

#[derive(Debug, Default, Deserialize)]
struct OwnerConfig {
    #[serde(default)]
    phone: Option<String>,
    #[serde(default)]
    emails: Vec<String>,
    #[serde(default)]
    source_dirs: Vec<PathBuf>,
}

fn resolve_optional_config(explicit: Option<PathBuf>, candidates: &[&str]) -> Option<PathBuf> {
    match explicit {
        Some(path) => Some(path),
        None => candidates
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file()),
    }
}

fn find_owner_config_path() -> Option<PathBuf> {
    const CANDIDATES: &[&str] = &[
        "config/owner.toml",
        "crates/sms-backup-plus-to-csv/config/owner.toml",
    ];
    CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

fn load_owner_config() -> Result<OwnerConfig> {
    let Some(path) = find_owner_config_path() else {
        return Ok(OwnerConfig::default());
    };
    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read owner config {}", path.display()))?;
    toml::from_str(&text)
        .with_context(|| format!("failed to parse owner config {}", path.display()))
}

fn resolve_owner(
    cli_phones: Vec<String>,
    cli_emails: Vec<String>,
) -> Result<(Vec<String>, Vec<String>, Vec<PathBuf>)> {
    let defaults = load_owner_config()?;
    let phones = if !cli_phones.is_empty() {
        cli_phones
    } else if let Some(phone) = defaults.phone {
        vec![phone]
    } else {
        anyhow::bail!(
            "owner phone required: pass --owner-phone or set phone in config/owner.toml"
        );
    };
    let emails = if !cli_emails.is_empty() {
        cli_emails
    } else if !defaults.emails.is_empty() {
        defaults.emails
    } else {
        anyhow::bail!(
            "owner email required: pass --owner-email or set emails in config/owner.toml"
        );
    };
    Ok((phones, emails, defaults.source_dirs))
}

fn resolve_inputs(cli_inputs: Vec<PathBuf>, defaults: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let inputs = if !cli_inputs.is_empty() {
        cli_inputs
    } else {
        defaults
    };
    if inputs.is_empty() {
        anyhow::bail!(
            "no --input given and config/owner.toml has no source_dirs; \
             pass --input PATH or set source_dirs in owner.toml"
        );
    }
    Ok(inputs)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Convert {
            input,
            output,
            owner_phones,
            owner_emails,
            contacts,
            name_mapping,
        } => {
            let (owner_phones, emails, default_inputs) =
                resolve_owner(owner_phones, owner_emails)?;
            let input = resolve_inputs(input, default_inputs)?;
            let contacts = resolve_optional_config(
                contacts,
                &["config/contacts.csv", "../../config/contacts.csv"],
            );
            let name_mapping = resolve_optional_config(
                name_mapping,
                &[
                    "config/name-mapping.csv",
                    "../../config/name-mapping.csv",
                    "crates/sms-backup-plus-to-csv/config/name-mapping.csv",
                ],
            );
            let report = convert_export(
                &input,
                &output,
                &owner_phones,
                &emails,
                contacts.as_deref(),
                name_mapping.as_deref(),
                cli.verbose,
            )?;

            if !cli.no_summary {
                println!("Wrote {}", output.display());
                println!("  conversations:     {}", report.conversations);
                println!("  flat EMLs:         {}", report.flat_eml);
                println!("  archive EMLs:      {}", report.archive_eml);
                println!("  messages (raw):    {}", report.messages_before_dedupe);
                println!("  messages (deduped):{}", report.messages);
                println!("  duplicates dropped:{}", report.duplicates_dropped);
                println!("  attachments:       {}", report.attachments_saved);
                println!("  sent / received:   {} / {}", report.sent, report.received);
                if report.skipped_invalid_date > 0 {
                    println!("  skipped bad date:  {}", report.skipped_invalid_date);
                }
                if report.skipped_not_sms_backup_plus > 0 {
                    println!(
                        "  not SMS Backup+:   {}",
                        report.skipped_not_sms_backup_plus
                    );
                }
                if !report.errors.is_empty() {
                    println!("  errors:            {}", report.errors.len());
                    for err in report.errors.iter().take(10) {
                        println!("    {err}");
                    }
                }
            }
        }
    }
    Ok(())
}
