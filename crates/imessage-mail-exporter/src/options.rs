//! Slim options for mail-archive export (no clap / HTML / CSV).

use std::path::{Path, PathBuf};

use imessage_database::{
    tables::table::DEFAULT_PATH_IOS,
    util::{platform::Platform, query_context::QueryContext},
};

/// Whether to embed attachment bytes in `.eml` files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentEmbed {
    /// Resolve and embed media bytes (macOS path or iOS decrypt).
    Embed,
    /// Skip media bytes (empty attachment parts still possible via other fields).
    Disabled,
}

/// Parsed options for one mail export run.
#[derive(Debug)]
pub struct MailOptions {
    pub db_path: PathBuf,
    pub attachment_root: Option<String>,
    pub export_path: PathBuf,
    pub query_context: QueryContext,
    pub use_caller_id: bool,
    pub platform: Platform,
    pub conversation_filter: Option<String>,
    pub cleartext_password: Option<String>,
    pub contacts_path: Option<PathBuf>,
    pub attachment_embed: AttachmentEmbed,
}

impl MailOptions {
    /// Messages database path for the selected platform.
    pub fn get_db_path(&self) -> PathBuf {
        match self.platform {
            Platform::iOS => self.db_path.join(DEFAULT_PATH_IOS),
            Platform::macOS => self.db_path.clone(),
        }
    }
}

/// Validate export directory does not already contain conversation `.eml` folders.
pub fn validate_export_path(export_path: &Path) -> Result<PathBuf, crate::error::RuntimeError> {
    use crate::error::RuntimeError;

    let resolved = export_path.to_path_buf();
    if resolved.exists() {
        match resolved.read_dir() {
            Ok(files) => {
                for file in files.flatten() {
                    let path = file.path();
                    if path.is_dir() {
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if name != "attachments" && dir_contains_eml(&path) {
                            return Err(RuntimeError::InvalidOptions(format!(
                                "Specified export path {} contains existing \"eml\" export data!",
                                resolved.display()
                            )));
                        }
                    }
                }
            }
            Err(why) => {
                return Err(RuntimeError::InvalidOptions(format!(
                    "Specified export path {} is not a valid directory: {why}",
                    resolved.display()
                )));
            }
        }
    }
    Ok(resolved)
}

fn dir_contains_eml(dir: &Path) -> bool {
    let Ok(entries) = dir.read_dir() else {
        return false;
    };
    entries.flatten().any(|e| {
        e.path()
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("eml"))
    })
}
