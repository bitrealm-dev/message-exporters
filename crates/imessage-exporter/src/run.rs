//! Library entrypoint for in-process iMessage export (GUI).
//!
//! Media convert/compress post-processing is left to the caller (GUI
//! `run_imessage_media_post`) so behavior matches the spawn-based path.

use std::path::PathBuf;

use imessage_database::util::{
    dirs::default_db_path, platform::Platform, query_context::QueryContext,
};
use message_exporters_core::{ApplePlatform, ExporterConfig, SourceConfig};

use crate::app::{
    compatibility::attachment_manager::{AttachmentManager, AttachmentManagerMode},
    error::RuntimeError,
    export_type::ExportType,
    options::{validate_path, Options},
    runtime::Config,
};
use crate::cancel::check_cancel;

/// Result of [`run`]. Export logging still goes to stdout/stderr via [`Config::start`].
#[derive(Debug, Default)]
pub struct RunResult {
    pub messages: Vec<String>,
}

/// Build [`Options`] from [`ExporterConfig`], then run the export.
pub fn run(config: &ExporterConfig) -> Result<RunResult, RuntimeError> {
    check_cancel(config.cancel.as_ref())?;
    let options = options_from_export_config(config)?;
    run_with_options(options)?;
    check_cancel(config.cancel.as_ref())?;
    Ok(RunResult {
        messages: Vec::new(),
    })
}

/// Shared path used by the CLI after [`Options::from_args`].
pub fn run_with_options(options: Options) -> Result<(), RuntimeError> {
    let mut app = Config::new(options)?;
    app.resolve_filtered_handles();
    app.start()
}

/// Convert library [`ExporterConfig`] into CLI [`Options`] (same validation rules).
pub fn options_from_export_config(config: &ExporterConfig) -> Result<Options, RuntimeError> {
    let SourceConfig::Apple(source) = &config.source else {
        return Err(RuntimeError::InvalidOptions(
            "imessage-exporter requires SourceConfig::Apple".to_string(),
        ));
    };
    let obfuscate = config.obfuscate_active();

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
            "--cleartext-password is enabled; it can only be used with iOS backups.".to_string(),
        ));
    }

    if let Some(path) = &source.attachment_root {
        let custom_attachment_path = PathBuf::from(path);
        if !custom_attachment_path.exists() {
            return Err(RuntimeError::InvalidOptions(format!(
                "Supplied --attachment-root `{path}` does not exist!"
            )));
        }
        if platform == Platform::iOS {
            eprintln!(
                "Option --attachment-root is enabled, but the platform is {}, so the root will have no effect!",
                Platform::iOS
            );
        }
    }

    if let Some(path) = &source.apple_contacts {
        if !path.exists() {
            return Err(RuntimeError::InvalidOptions(format!(
                "Supplied --contacts-path `{}` does not exist!",
                path.display()
            )));
        }
        if platform == Platform::iOS {
            eprintln!(
                "Option --contacts-path is enabled, but the platform is {}, so the path will have no effect!",
                Platform::iOS
            );
        }
    }

    let attachment_manager_mode = AttachmentManagerMode::from_cli(&source.copy_method).ok_or(
        RuntimeError::InvalidOptions(format!(
            "{} is not a valid attachment manager mode! Must be one of <clone, basic, full, disabled>",
            source.copy_method
        )),
    )?;

    let export_path_str = config.output.to_string_lossy().into_owned();
    let export_path = validate_path(Some(&export_path_str), Some(&ExportType::Csv))?;

    Ok(Options {
        db_path,
        attachment_root: source.attachment_root.clone(),
        attachment_manager: AttachmentManager::from(attachment_manager_mode),
        diagnostic: false,
        export_type: Some(ExportType::Csv),
        export_path,
        query_context,
        no_lazy: false,
        custom_name: None,
        use_caller_id: source.use_caller_id,
        platform,
        ignore_disk_space: source.ignore_disk_space,
        conversation_filter: source.conversation_filter.clone(),
        cleartext_password: source.backup_password.clone(),
        contacts_path: source.apple_contacts.clone(),
        show_progress: source.show_progress,
        obfuscate,
        obfuscate_seed: config.obfuscate.seed.clone(),
    })
}
