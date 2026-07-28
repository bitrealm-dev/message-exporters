//! Shared typed export configuration for CLI, library, and GUI.
//!
//! [`ExporterConfig`] holds options common to (nearly) every exporter.
//! Exporter-specific knobs live in [`SourceConfig`].

use std::path::{Path, PathBuf};

use message_csv::DateRange;
use message_media::{CompressOptions, MediaMode};

use crate::exporters::{ApplePlatform, ContactsKind, Exporter, WhatsappPlatform};
use crate::process::CancelFlag;

/// Shared export inputs. Source-specific fields are in [`Self::source`].
#[derive(Debug, Clone)]
pub struct ExporterConfig {
    /// Input paths (usually one). SMS Backup+ CLI may pass several; WhatsApp may leave empty.
    pub inputs: Vec<PathBuf>,
    pub output: PathBuf,
    pub date_range: DateRange,
    pub contacts: Option<ContactsConfig>,
    pub obfuscate: ObfuscateConfig,
    /// OpenExtract uses [`MediaMode::Disabled`].
    pub media: MediaConfig,
    pub cancel: Option<CancelFlag>,
    pub source: SourceConfig,
}

impl ExporterConfig {
    /// First input path, if any.
    pub fn primary_input(&self) -> Option<&Path> {
        self.inputs.first().map(PathBuf::as_path)
    }

    /// Require a single primary input (most exporters).
    pub fn require_input(&self) -> Result<&Path, String> {
        match self.inputs.as_slice() {
            [path] => Ok(path.as_path()),
            [] => Err("input is required".into()),
            _ => Err("expected a single input path".into()),
        }
    }

    /// Split contacts into `(--contacts, --vcf)` paths for loaders that take both.
    pub fn contacts_csv_vcf(&self) -> (Option<PathBuf>, Option<PathBuf>) {
        match &self.contacts {
            Some(c) => c.csv_and_vcf(),
            None => (None, None),
        }
    }

    pub fn obfuscate_active(&self) -> bool {
        self.obfuscate.enabled || self.obfuscate.seed.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct ContactsConfig {
    pub path: PathBuf,
    pub kind: ContactsKind,
}

impl ContactsConfig {
    pub fn csv_and_vcf(&self) -> (Option<PathBuf>, Option<PathBuf>) {
        match self.kind {
            ContactsKind::Csv => (Some(self.path.clone()), None),
            ContactsKind::Vcf => (None, Some(self.path.clone())),
            ContactsKind::None => (None, None),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ObfuscateConfig {
    pub enabled: bool,
    pub seed: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MediaConfig {
    pub mode: MediaMode,
    pub compress: CompressOptions,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            mode: MediaMode::Clone,
            compress: CompressOptions::default(),
        }
    }
}

/// Exporter-specific options. Exactly one variant is set per run.
#[derive(Debug, Clone)]
pub enum SourceConfig {
    GoSmsPro(GoSmsProConfig),
    SmsBackupRestore(SmsBackupRestoreConfig),
    SmsBackupPlus(SmsBackupPlusConfig),
    OpenExtract(OpenExtractConfig),
    Imazing(ImazingConfig),
    Apple(AppleConfig),
    Whatsapp(WhatsappConfig),
}

impl SourceConfig {
    pub fn exporter(&self) -> Exporter {
        match self {
            Self::GoSmsPro(_) => Exporter::GoSmsPro,
            Self::SmsBackupRestore(_) => Exporter::SmsBackupRestore,
            Self::SmsBackupPlus(_) => Exporter::SmsBackupPlus,
            Self::OpenExtract(_) => Exporter::OpenExtract,
            Self::Imazing(_) => Exporter::Imazing,
            Self::Apple(_) => Exporter::Imessage,
            Self::Whatsapp(_) => Exporter::Whatsapp,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GoSmsProConfig {
    pub owner_phones: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SmsBackupRestoreConfig {
    pub owner_phones: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SmsBackupPlusConfig {
    pub owner_phones: Vec<String>,
    pub owner_emails: Vec<String>,
    pub name_mapping: Option<PathBuf>,
    pub verbose: bool,
    pub include_summary: bool,
}

#[derive(Debug, Clone, Default)]
pub struct OpenExtractConfig {}

#[derive(Debug, Clone, Default)]
pub struct ImazingConfig {
    /// Fixed UTC offset for date midnight, e.g. `UTC-05:00`.
    pub timezone: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppleConfig {
    pub platform: Option<ApplePlatform>,
    pub attachment_root: Option<String>,
    /// `disabled`, `clone`, `basic`, or `full`.
    pub copy_method: String,
    pub apple_contacts: Option<PathBuf>,
    pub backup_password: Option<String>,
    pub conversation_filter: Option<String>,
    /// Raw `YYYY-MM-DD` for QueryContext (DateRange does not retain strings).
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub use_caller_id: bool,
    pub show_progress: bool,
    pub ignore_disk_space: bool,
}

impl Default for AppleConfig {
    fn default() -> Self {
        Self {
            platform: None,
            attachment_root: None,
            copy_method: "clone".into(),
            apple_contacts: None,
            backup_password: None,
            conversation_filter: None,
            start_date: None,
            end_date: None,
            use_caller_id: true,
            show_progress: false,
            ignore_disk_space: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WhatsappConfig {
    pub platform: Option<WhatsappPlatform>,
    pub json: Option<PathBuf>,
    pub key: Option<String>,
    pub backup: Option<PathBuf>,
    pub wa: Option<PathBuf>,
    pub media: Option<PathBuf>,
    pub db: Option<PathBuf>,
    pub business: bool,
}
