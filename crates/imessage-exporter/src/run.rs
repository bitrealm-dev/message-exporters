//! Library entrypoint for in-process iMessage export (GUI).
//!
//! Media convert/compress post-processing is left to the caller (GUI
//! `run_imessage_media_post`) so behavior matches the spawn-based path.

use std::path::PathBuf;

use imessage_database::util::{
    dirs::default_db_path, platform::Platform, query_context::QueryContext,
};

use crate::app::{
    compatibility::attachment_manager::{AttachmentManager, AttachmentManagerMode},
    error::RuntimeError,
    export_type::ExportType,
    options::{validate_path, Options},
    runtime::Config,
};
use crate::cancel::{check_cancel, CancelFlag};

/// Inputs for an in-process iMessage CSV export (mirrors GUI / Options fields).
#[derive(Debug, Clone)]
pub struct ExportConfig {
    pub db_path: PathBuf,
    pub export_path: PathBuf,
    /// `"macOS"` / `"iOS"`; when `None`, platform is auto-detected from `db_path`.
    pub platform: Option<String>,
    pub attachment_root: Option<String>,
    /// Attachment copy mode: `disabled`, `clone`, `basic`, or `full`.
    /// GUI convert/compress maps to `clone` here; media post stays in the GUI.
    pub copy_method: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub conversation_filter: Option<String>,
    pub contacts_path: Option<PathBuf>,
    pub cleartext_password: Option<String>,
    pub use_caller_id: bool,
    pub obfuscate: bool,
    pub obfuscate_seed: Option<String>,
    /// Checked at start and end of [`run`]. The export loop does not poll mid-run.
    pub cancel: Option<CancelFlag>,
    /// When false (default for library/GUI), suppress the progress bar.
    pub show_progress: bool,
    /// Bypass free-disk-space check (CLI `--ignore-disk-warning`).
    pub ignore_disk_space: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::new(),
            export_path: PathBuf::new(),
            platform: None,
            attachment_root: None,
            copy_method: AttachmentManagerMode::default().to_string(),
            start_date: None,
            end_date: None,
            conversation_filter: None,
            contacts_path: None,
            cleartext_password: None,
            use_caller_id: false,
            obfuscate: false,
            obfuscate_seed: None,
            cancel: None,
            show_progress: false,
            ignore_disk_space: false,
        }
    }
}

/// Result of [`run`]. Export logging still goes to stdout/stderr via [`Config::start`].
#[derive(Debug, Default)]
pub struct RunResult {
    pub messages: Vec<String>,
}

/// Build [`Options`] from [`ExportConfig`], then run the export.
pub fn run(config: &ExportConfig) -> Result<RunResult, RuntimeError> {
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

/// Convert library [`ExportConfig`] into CLI [`Options`] (same validation rules).
pub fn options_from_export_config(config: &ExportConfig) -> Result<Options, RuntimeError> {
    let obfuscate = config.obfuscate || config.obfuscate_seed.is_some();

    let mut query_context = QueryContext::default();
    if let Some(start) = &config.start_date
        && let Err(why) = query_context.set_start(start)
    {
        return Err(RuntimeError::InvalidOptions(format!("{why}")));
    }
    if let Some(end) = &config.end_date
        && let Err(why) = query_context.set_end(end)
    {
        return Err(RuntimeError::InvalidOptions(format!("{why}")));
    }

    let db_path = if config.db_path.as_os_str().is_empty() {
        default_db_path()
    } else {
        config.db_path.clone()
    };
    let platform = match &config.platform {
        Some(platform_str) => Platform::from_cli(platform_str).ok_or(
            RuntimeError::InvalidOptions(format!(
                "{platform_str} is not a valid platform! Must be one of <macOS, iOS>"
            )),
        )?,
        None => Platform::determine(&db_path)?,
    };

    if config.cleartext_password.is_some() && !matches!(platform, Platform::iOS) {
        return Err(RuntimeError::InvalidOptions(
            "--cleartext-password is enabled; it can only be used with iOS backups.".to_string(),
        ));
    }

    if let Some(path) = &config.attachment_root {
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

    if let Some(path) = &config.contacts_path {
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

    let attachment_manager_mode = AttachmentManagerMode::from_cli(&config.copy_method).ok_or(
        RuntimeError::InvalidOptions(format!(
            "{} is not a valid attachment manager mode! Must be one of <clone, basic, full, disabled>",
            config.copy_method
        )),
    )?;

    let export_path_str = config.export_path.to_string_lossy().into_owned();
    let export_path = validate_path(Some(&export_path_str), Some(&ExportType::Csv))?;

    Ok(Options {
        db_path,
        attachment_root: config.attachment_root.clone(),
        attachment_manager: AttachmentManager::from(attachment_manager_mode),
        diagnostic: false,
        export_type: Some(ExportType::Csv),
        export_path,
        query_context,
        no_lazy: false,
        custom_name: None,
        use_caller_id: config.use_caller_id,
        platform,
        ignore_disk_space: config.ignore_disk_space,
        conversation_filter: config.conversation_filter.clone(),
        cleartext_password: config.cleartext_password.clone(),
        contacts_path: config.contacts_path.clone(),
        show_progress: config.show_progress,
        obfuscate,
        obfuscate_seed: config.obfuscate_seed.clone(),
    })
}
