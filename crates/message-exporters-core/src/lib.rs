//! Shared exporter forms, typed config, and process helpers for desktop GUIs.

mod config;
mod export_ini;
mod exporters;
mod process;

pub use config::{
    AppleConfig, ContactsConfig, ExporterConfig, GoSmsProConfig, ImazingConfig, MediaConfig,
    MessageReexportConfig, OUTPUT_FORMATS_MAIL, ObfuscateConfig, OpenExtractConfig, OutputFormat,
    SmsBackupPlusConfig, SmsBackupRestoreConfig, SourceConfig, WhatsappConfig,
};
pub use export_ini::{
    EXPORT_INI_NAME, ExportIniState, ExporterSection, ReexportSection, resolve_export_ini_path,
};
pub use exporters::{
    APPLE_PLATFORMS, ATTACHMENT_MEDIA, ApplePlatform, AttachmentMedia, CONTACT_KINDS, ContactsKind,
    EXPORTERS, Exporter, Form, MAX_RESOLUTIONS, WHATSAPP_PLATFORMS, WhatsappPlatform,
    contacts_kind_from_path, ensure_output_dir,
};
pub use process::{
    CancelFlag, ProcessControl, ProcessEvent, check_cancel, is_cancelled, resolve_binary, spawn,
    spawn_job,
};
