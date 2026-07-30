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
pub use export_ini::{ExportIniState, ReexportSection, VaultSection};
pub use exporters::{
    APPLE_PLATFORMS, ATTACHMENT_MEDIA, ApplePlatform, AttachmentMedia, ContactsKind, EXPORTERS,
    Exporter, Form, MAX_RESOLUTIONS, WHATSAPP_PLATFORMS, WhatsappPlatform, contacts_kind_from_path,
    ensure_output_dir,
};
pub use process::{
    CancelFlag, LogSink, ProcessControl, ProcessEvent, check_cancel, emit_log, is_cancelled,
    resolve_binary, spawn, spawn_job,
};
