//! Shared exporter forms, typed config, and process helpers for desktop GUIs.

mod config;
mod export_ini;
mod exporters;
mod process;

pub use config::{
    AppleConfig, ContactsConfig, ExporterConfig, GoSmsProConfig, ImazingConfig, MediaConfig,
    ObfuscateConfig, OpenExtractConfig, OutputFormat, SmsBackupPlusConfig, SmsBackupRestoreConfig,
    SourceConfig, WhatsappConfig, OUTPUT_FORMATS,
};
pub use export_ini::{
    resolve_export_ini_path, ExportIniState, ExporterSection, EXPORT_INI_NAME,
};
pub use exporters::{
    contacts_kind_from_path, ensure_output_dir, ApplePlatform, AttachmentMedia, ContactsKind,
    Exporter, Form, WhatsappPlatform, APPLE_PLATFORMS, ATTACHMENT_MEDIA, CONTACT_KINDS, EXPORTERS,
    MAX_RESOLUTIONS, WHATSAPP_PLATFORMS,
};
pub use process::{
    check_cancel, is_cancelled, resolve_binary, spawn, spawn_job, CancelFlag, ProcessControl,
    ProcessEvent,
};
