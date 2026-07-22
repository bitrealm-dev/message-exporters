//! Shared exporter forms and CLI process helpers for desktop GUIs.

mod exporters;
mod process;

pub use exporters::{
    default_output_dir, ApplePlatform, AttachmentMedia, ContactsKind, Exporter, Form,
    APPLE_PLATFORMS, ATTACHMENT_MEDIA, CONTACT_KINDS, EXPORTERS, MAX_RESOLUTIONS,
};
pub use process::{resolve_binary, spawn, ProcessControl, ProcessEvent};
