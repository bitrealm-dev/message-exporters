use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use sms_backup_plus_exporter::{convert_export, dedupe_eml};

#[derive(Parser, Debug)]
#[command(name = "sms-backup-plus-exporter")]
#[command(about = "Convert or dedupe SMS Backup+ EML exports")]
struct Cli {
    /// Print progress while scanning and writing (every 5000 items)
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
    /// Convert EML tree to SMS NDJSON (`message_json::sms`)
    Convert {
        /// Path to a .eml file or directory tree of EMLs (Archive/, Sent/, …).
        /// Repeat for multiple roots; trees are merged and path-deduped.
        /// Default: source_dirs from config/owner.toml when set.
        #[arg(long = "input")]
        input: Vec<PathBuf>,

        /// Output directory for NDJSON + attachments/
        #[arg(long)]
        output: PathBuf,

        /// Owner phone (E.164 or digits). Default: config/owner.toml
        #[arg(long)]
        owner_phone: Option<String>,

        /// Owner email addresses used to detect sent messages when X-smssync-type is missing.
        /// Default: config/owner.toml
        #[arg(long = "owner-email", value_name = "EMAIL")]
        owner_emails: Vec<String>,

        /// Contacts CSV (phones,first_name,last_name,…) for name→phone lookup.
        /// Default: vault config/contacts.csv when that file exists.
        #[arg(long)]
        contacts: Option<PathBuf>,

        /// Name mapping CSV (correct_name,incorrect_name) for EML export aliases.
        /// Default: repo config/name-mapping.csv when that file exists.
        #[arg(long = "name-mapping")]
        name_mapping: Option<PathBuf>,
    },
    /// Deduplicate flat EMLs into year folders with useful filenames
    DedupeEml {
        /// Path to a messy .eml file or directory tree.
        /// Repeat for multiple roots; trees are merged and path-deduped.
        /// Default: source_dirs from config/owner.toml when set.
        #[arg(long = "input")]
        input: Vec<PathBuf>,

        /// Output directory for unique flat .eml files
        #[arg(long)]
        output: PathBuf,

        /// Owner phone (E.164 or digits). Default: config/owner.toml
        #[arg(long)]
        owner_phone: Option<String>,

        /// Owner email addresses used to detect sent messages when X-smssync-type is missing.
        /// Default: config/owner.toml
        #[arg(long = "owner-email", value_name = "EMAIL")]
        owner_emails: Vec<String>,

        /// Contacts CSV (phones,first_name,last_name,…) for name→phone lookup.
        /// Default: vault config/contacts.csv when that file exists.
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
    /// Default --input roots when the CLI omits --input.
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
        "crates/sms-backup-plus-exporter/config/owner.toml",
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
    cli_phone: Option<String>,
    cli_emails: Vec<String>,
) -> Result<(String, Vec<String>, Vec<PathBuf>)> {
    let defaults = load_owner_config()?;
    let phone = cli_phone
        .or(defaults.phone)
        .unwrap_or_else(|| "+15555550100".to_string());
    let emails = if !cli_emails.is_empty() {
        cli_emails
    } else if !defaults.emails.is_empty() {
        defaults.emails
    } else {
        vec!["owner@example.com".to_string()]
    };
    Ok((phone, emails, defaults.source_dirs))
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
            owner_phone,
            owner_emails,
            contacts,
            name_mapping,
        } => {
            let (owner_phone, emails, default_inputs) =
                resolve_owner(owner_phone, owner_emails)?;
            let input = resolve_inputs(input, default_inputs)?;
            let contacts = resolve_optional_config(
                contacts,
                &[
                    "config/contacts.csv",
                    "../../config/contacts.csv",
                ],
            );
            let name_mapping = resolve_optional_config(
                name_mapping,
                &[
                    "config/name-mapping.csv",
                    "../../config/name-mapping.csv",
                    "crates/sms-backup-plus-exporter/config/name-mapping.csv",
                ],
            );
            let report = convert_export(
                &input,
                &output,
                &[owner_phone],
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
                println!("  messages:          {}", report.messages);
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
        Commands::DedupeEml {
            input,
            output,
            owner_phone,
            owner_emails,
            contacts,
            name_mapping,
        } => {
            let (owner_phone, emails, default_inputs) =
                resolve_owner(owner_phone, owner_emails)?;
            let input = resolve_inputs(input, default_inputs)?;
            let contacts = resolve_optional_config(
                contacts,
                &[
                    "config/contacts.csv",
                    "../../config/contacts.csv",
                ],
            );
            let name_mapping = resolve_optional_config(
                name_mapping,
                &[
                    "config/name-mapping.csv",
                    "../../config/name-mapping.csv",
                    "crates/sms-backup-plus-exporter/config/name-mapping.csv",
                ],
            );
            let report = dedupe_eml(
                &input,
                &output,
                &owner_phone,
                &emails,
                contacts.as_deref(),
                name_mapping.as_deref(),
                cli.verbose,
            )?;

            if !cli.no_summary {
                println!("Wrote {}", output.display());
                if let Some(ref log_path) = report.log_path {
                    println!("  log:                 {}", log_path.display());
                }

                println!();
                println!("  Flat (one SMS per .eml file)");
                println!("  seen:                {}", report.flat_seen);
                println!("  unique:              {}", report.unique_flat);
                println!("  copied:              {}", report.copied);
                println!("  duplicates dropped:  {}", report.duplicates_dropped);

                println!();
                println!("  Archive (many SMS in one .eml file)");
                println!("  archive files:       {}", report.archive_eml);
                println!("  overlaps (in flat):  {}", report.archive_overlaps);
                println!("  archive-only msgs:   {}", report.archive_only);
                println!("  generated flats:     {}", report.archive_generated);
                if report.archive_generated_junk > 0 {
                    println!("  generated junk/:     {}", report.archive_generated_junk);
                }
                if report.flat_unknown_junk > 0 {
                    println!("  unknown-phone junk/: {}", report.flat_unknown_junk);
                }

                println!();
                println!("  Contacts");
                println!("  unique names mapped: {}", report.names_mapped);
                println!("  unique contacts:     {}", report.contacts_resolved);
                if report.unresolved_names > 0 {
                    println!("  unresolved names:    {}", report.unresolved_names);
                    if let Some(ref path) = report.unresolved_names_path {
                        println!("  unresolved list:     {}", path.display());
                    }
                }

                if report.skipped_not_sms > 0 || report.skipped_unparseable > 0 {
                    println!();
                    println!("  Skipped");
                    if report.skipped_not_sms > 0 {
                        println!("  not SMS:             {}", report.skipped_not_sms);
                    }
                    if report.skipped_unparseable > 0 {
                        println!("  unparseable:         {}", report.skipped_unparseable);
                        if let Some(ref path) = report.unparseable_dir {
                            println!("  unparseable dir:     {}", path.display());
                        }
                    }
                }

                if !report.errors.is_empty() {
                    println!();
                    println!("  errors:              {}", report.errors.len());
                    for err in report.errors.iter().take(10) {
                        println!("    {err}");
                    }
                }
            }
        }
    }
    Ok(())
}
