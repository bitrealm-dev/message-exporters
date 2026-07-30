//! Small helpers for reading `application/x-www-form-urlencoded` bodies
//! (decoded by axum into a `HashMap<String, String>`) into
//! `message_exporters_core::Form` fields, mirroring the field-by-field
//! translation the native GUI does between widgets and `Form`.

use std::collections::HashMap;

use message_exporters_core::{
    ApplePlatform, AttachmentMedia, ContactsKind, Exporter, OutputFormat, WhatsappPlatform,
    contacts_kind_from_path,
};
use message_media::MaxResolution;

pub type Params = HashMap<String, String>;

pub fn text(params: &Params, key: &str) -> String {
    params.get(key).cloned().unwrap_or_default()
}

/// HTML omits unchecked checkboxes from the submitted body entirely, so
/// presence (regardless of value) means "checked".
pub fn checkbox(params: &Params, key: &str) -> bool {
    params.contains_key(key)
}

pub fn exporter(params: &Params, key: &str) -> Option<Exporter> {
    params.get(key).and_then(|value| Exporter::from_ini_key(value))
}

pub fn output_format(params: &Params, key: &str, default: OutputFormat) -> OutputFormat {
    params
        .get(key)
        .and_then(|value| OutputFormat::parse(value).ok())
        .unwrap_or(default)
}

pub fn attachment_media(params: &Params, key: &str, default: AttachmentMedia) -> AttachmentMedia {
    params
        .get(key)
        .and_then(|value| AttachmentMedia::from_ini_str(value))
        .unwrap_or(default)
}

pub fn max_resolution(params: &Params, key: &str, default: MaxResolution) -> MaxResolution {
    params
        .get(key)
        .and_then(|value| MaxResolution::parse(value))
        .unwrap_or(default)
}

pub fn apple_platform(params: &Params, key: &str, default: ApplePlatform) -> ApplePlatform {
    params
        .get(key)
        .and_then(|value| ApplePlatform::from_ini_str(value))
        .unwrap_or(default)
}

pub fn whatsapp_platform(params: &Params, key: &str, default: WhatsappPlatform) -> WhatsappPlatform {
    params
        .get(key)
        .and_then(|value| WhatsappPlatform::from_ini_str(value))
        .unwrap_or(default)
}

/// Update `form.contacts_kind` from `form.contacts`, mirroring
/// `ui_contacts`'s call to `contacts_kind_from_path` after every edit.
pub fn refresh_contacts_kind(contacts: &str) -> ContactsKind {
    contacts_kind_from_path(contacts)
}
