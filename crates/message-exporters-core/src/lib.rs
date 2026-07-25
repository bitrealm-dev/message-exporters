//! Shared exporter forms and CLI process helpers for desktop GUIs.

mod export_ini;
mod exporters;
mod process;

pub use export_ini::{
    resolve_export_ini_path, ExportIniState, ExporterSection, EXPORT_INI_NAME,
};
pub use exporters::{
    contacts_kind_from_path, ensure_output_dir, ApplePlatform, AttachmentMedia, ContactsKind,
    Exporter, Form, WhatsappPlatform, APPLE_PLATFORMS, ATTACHMENT_MEDIA, CONTACT_KINDS, EXPORTERS,
    MAX_RESOLUTIONS, WHATSAPP_PLATFORMS,
};
pub use process::{resolve_binary, spawn, ProcessControl, ProcessEvent};
