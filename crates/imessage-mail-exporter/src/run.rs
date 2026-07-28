//! Library entrypoint: [`ExporterConfig`] → mail archive.

use std::path::PathBuf;

use imessage_database::util::{
    dirs::default_db_path, platform::Platform, query_context::QueryContext,
};
use message_exporters_core::{ApplePlatform, ExporterConfig, OutputFormat, SourceConfig};

use crate::{
    emit::run_export,
    error::RuntimeError,
    options::{validate_export_path, AttachmentEmbed, MailOptions},
    session::MailSession,
};

/// Result of [`run`]. Logging goes to stderr during export.
#[derive(Debug, Default)]
pub struct RunResult {
    pub messages: Vec<String>,
}

/// Build options from [`ExporterConfig`], open the DB, and write `.eml` archives.
pub fn run(config: &ExporterConfig) -> Result<RunResult, RuntimeError> {
    check_cancel(config)?;
    if config.output_format != OutputFormat::Eml {
        return Err(RuntimeError::InvalidOptions(
            "imessage-mail-exporter requires OutputFormat::Eml (CSV stays on imessage-exporter)"
                .to_string(),
        ));
    }

    let options = options_from_export_config(config)?;
    let mut session = MailSession::new(options)?;
    session.resolve_filtered_handles();
    check_cancel(config)?;
    run_export(&session)?;
    check_cancel(config)?;

    Ok(RunResult {
        messages: vec![format!(
            "Wrote eml archive under {}",
            config.output.display()
        )],
    })
}

fn check_cancel(config: &ExporterConfig) -> Result<(), RuntimeError> {
    message_exporters_core::check_cancel(config.cancel.as_ref())
        .map_err(|msg| RuntimeError::InvalidOptions(msg.to_string()))
}

fn options_from_export_config(config: &ExporterConfig) -> Result<MailOptions, RuntimeError> {
    let SourceConfig::Apple(source) = &config.source else {
        return Err(RuntimeError::InvalidOptions(
            "imessage-mail-exporter requires SourceConfig::Apple".to_string(),
        ));
    };

    let mut query_context = QueryContext::default();
    if let Some(start) = &source.start_date
        && let Err(why) = query_context.set_start(start)
    {
        return Err(RuntimeError::InvalidOptions(format!("{why}")));
    }
    if let Some(end) = &source.end_date
        && let Err(why) = query_context.set_end(end)
    {
        return Err(RuntimeError::InvalidOptions(format!("{why}")));
    }

    let db_path = match config.primary_input() {
        Some(path) if !path.as_os_str().is_empty() => path.to_path_buf(),
        _ => default_db_path(),
    };
    let platform = match source.platform {
        Some(ApplePlatform::MacOs) => Platform::from_cli("macOS").ok_or_else(|| {
            RuntimeError::InvalidOptions(
                "macOS is not a valid platform! Must be one of <macOS, iOS>".to_string(),
            )
        })?,
        Some(ApplePlatform::Ios) => Platform::from_cli("iOS").ok_or_else(|| {
            RuntimeError::InvalidOptions(
                "iOS is not a valid platform! Must be one of <macOS, iOS>".to_string(),
            )
        })?,
        Some(ApplePlatform::Auto) | None => Platform::determine(&db_path)?,
    };

    if source.backup_password.is_some() && !matches!(platform, Platform::iOS) {
        return Err(RuntimeError::InvalidOptions(
            "backup password is enabled; it can only be used with iOS backups.".to_string(),
        ));
    }

    if let Some(path) = &source.attachment_root {
        let custom_attachment_path = PathBuf::from(path);
        if !custom_attachment_path.exists() {
            return Err(RuntimeError::InvalidOptions(format!(
                "Supplied attachment-root `{path}` does not exist!"
            )));
        }
        if platform == Platform::iOS {
            eprintln!(
                "Option attachment-root is enabled, but the platform is {}, so the root will have no effect!",
                Platform::iOS
            );
        }
    }

    if let Some(path) = &source.apple_contacts {
        if !path.exists() {
            return Err(RuntimeError::InvalidOptions(format!(
                "Supplied contacts path `{}` does not exist!",
                path.display()
            )));
        }
        if platform == Platform::iOS {
            eprintln!(
                "Option contacts path is enabled, but the platform is {}, so the path will have no effect!",
                Platform::iOS
            );
        }
    }

    let attachment_embed = match source.copy_method.to_ascii_lowercase().as_str() {
        "disabled" => AttachmentEmbed::Disabled,
        "clone" | "basic" | "full" => AttachmentEmbed::Embed,
        other => {
            return Err(RuntimeError::InvalidOptions(format!(
                "{other} is not a valid attachment mode! Must be one of <clone, basic, full, disabled>"
            )));
        }
    };

    let export_path = validate_export_path(&config.output)?;
    std::fs::create_dir_all(&export_path)?;

    Ok(MailOptions {
        db_path,
        attachment_root: source.attachment_root.clone(),
        export_path,
        query_context,
        use_caller_id: source.use_caller_id,
        platform,
        conversation_filter: source.conversation_filter.clone(),
        cleartext_password: source.backup_password.clone(),
        contacts_path: source.apple_contacts.clone(),
        attachment_embed,
    })
}
