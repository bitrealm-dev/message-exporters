use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use message_anonymize::{anonymize_near_vault_dir, resolve_anonymizer};
use message_contacts::{resolve_contacts_cli, NameMapping};
use serde::Deserialize;
use sms_backup_plus_out::convert_export;

#[derive(Parser, Debug)]
#[command(name = "sms-backup-plus-out")]
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
        /// Default: `phones` in config/owner.toml
        #[arg(long = "owner-phone")]
        owner_phones: Vec<String>,

        /// Owner email addresses used to detect sent messages when X-smssync-type is missing.
        /// Default: `emails` in config/owner.toml
        #[arg(long = "owner-email", value_name = "EMAIL")]
        owner_emails: Vec<String>,

        /// Vault-shaped contacts CSV (phones,first_name,last_name,…) for name↔phone lookup.
        /// Optional; without it (or `--vcf`) phone numbers are not resolved to names.
        #[arg(long)]
        contacts: Option<PathBuf>,

        /// Contacts VCF (alternate to `--contacts`).
        #[arg(long)]
        vcf: Option<PathBuf>,

        /// Name mapping CSV (correct_name,incorrect_name) for EML export aliases.
        /// Default: config/name-mapping.csv when that file exists.
        #[arg(long = "name-mapping")]
        name_mapping: Option<PathBuf>,

        /// Rewrite output with stable, non-reversible fake names/numbers/text and placeholder media
        #[arg(long)]
        anonymize: bool,

        /// Optional 64-char hex seed for reproducible anonymization (implies --anonymize)
        #[arg(long = "anonymize-seed")]
        anonymize_seed: Option<String>,
    },
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerConfig {
    /// One or more owner numbers (same meaning as repeated `--owner-phone`).
    #[serde(default)]
    phones: Vec<String>,
    #[serde(default)]
    emails: Vec<String>,
    #[serde(default)]
    source_dirs: Vec<PathBuf>,
}

fn crate_config(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("config")
        .join(name)
}

fn resolve_optional_config(explicit: Option<PathBuf>, default_name: &str) -> Option<PathBuf> {
    match explicit {
        Some(path) => Some(path),
        None => {
            let path = crate_config(default_name);
            path.is_file().then_some(path)
        }
    }
}

fn find_owner_config_path() -> Option<PathBuf> {
    let path = crate_config("owner.toml");
    path.is_file().then_some(path)
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
    } else if !defaults.phones.is_empty() {
        defaults.phones
    } else {
        anyhow::bail!(
            "owner phone required: pass --owner-phone or set phones in config/owner.toml"
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
            vcf,
            name_mapping,
            anonymize,
            anonymize_seed,
        } => {
            let (owner_phones, emails, default_inputs) =
                resolve_owner(owner_phones, owner_emails)?;
            let input = resolve_inputs(input, default_inputs)?;
            let (contacts_book, contacts_path) = resolve_contacts_cli(contacts, vcf)?;
            let name_mapping_path = resolve_optional_config(name_mapping, "name-mapping.csv");
            let (name_mapping, _) = NameMapping::load_optional(name_mapping_path.as_deref())?;
            if cli.verbose {
                match contacts_path.as_ref() {
                    Some(path) => eprintln!("contacts: {}", path.display()),
                    None => eprintln!("contacts: (none)"),
                }
            }
            let report = convert_export(
                &input,
                &output,
                &owner_phones,
                &emails,
                &contacts_book,
                &name_mapping,
                cli.verbose,
            )?;

            if anonymize || anonymize_seed.is_some() {
                let mut anon = resolve_anonymizer(anonymize_seed.as_deref())?;
                let n = anonymize_near_vault_dir(&output, &mut anon)?;
                eprintln!("Anonymized {n} CSV file(s) under {}", output.display());
            }

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
                if report.unknown_chat_messages > 0 {
                    println!(
                        "  unknown chat rows: {}",
                        report.unknown_chat_messages
                    );
                }
                if report.skipped_not_sms_backup_plus > 0 {
                    println!(
                        "  not SMS Backup+:   {}",
                        report.skipped_not_sms_backup_plus
                    );
                }
                if report.skipped_parse_error > 0 {
                    println!("  parse errors:      {}", report.skipped_parse_error);
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
