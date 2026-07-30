//! `<select>` option lists, built server-side so templates never need to
//! reason about enum internals (keeps the `.html` files plain HTML).

use message_exporters_core::{
    APPLE_PLATFORMS, ATTACHMENT_MEDIA, ApplePlatform, AttachmentMedia, EXPORTERS, Exporter,
    OUTPUT_FORMATS_MAIL, OutputFormat, WHATSAPP_PLATFORMS, WhatsappPlatform,
};
use message_media::MaxResolution;

const MAX_RESOLUTIONS: [MaxResolution; 3] = [
    MaxResolution::P720,
    MaxResolution::P1080,
    MaxResolution::P4k,
];

#[derive(Debug, Clone)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    pub selected: bool,
    /// Render a `<hr>` separator immediately before this option (used to set
    /// off experimental exporters, matching the native GUI's combo box).
    pub separator_before: bool,
}

fn opt(value: impl Into<String>, label: impl Into<String>, selected: bool) -> SelectOption {
    SelectOption {
        value: value.into(),
        label: label.into(),
        selected,
        separator_before: false,
    }
}

pub fn exporters(current: Exporter) -> Vec<SelectOption> {
    let mut saw_experimental = false;
    EXPORTERS
        .iter()
        .map(|&exporter| {
            let mut option = opt(
                exporter.ini_key(),
                exporter.dropdown_label(),
                exporter == current,
            );
            if !exporter.is_supported() && !saw_experimental {
                option.separator_before = true;
                saw_experimental = true;
            }
            option
        })
        .collect()
}

pub fn output_formats(current: OutputFormat) -> Vec<SelectOption> {
    OUTPUT_FORMATS_MAIL
        .iter()
        .map(|&format| opt(format.as_str(), format.to_string(), format == current))
        .collect()
}

pub fn attachment_media(current: AttachmentMedia) -> Vec<SelectOption> {
    ATTACHMENT_MEDIA
        .iter()
        .map(|&mode| opt(mode.as_ini_str(), mode.to_string(), mode == current))
        .collect()
}

pub fn max_resolutions(current: MaxResolution) -> Vec<SelectOption> {
    MAX_RESOLUTIONS
        .iter()
        .map(|&res| opt(res.as_str(), res.to_string(), res == current))
        .collect()
}

pub fn apple_platforms(current: ApplePlatform) -> Vec<SelectOption> {
    APPLE_PLATFORMS
        .iter()
        .map(|&platform| opt(platform.as_ini_str(), platform.to_string(), platform == current))
        .collect()
}

pub fn whatsapp_platforms(current: WhatsappPlatform) -> Vec<SelectOption> {
    WHATSAPP_PLATFORMS
        .iter()
        .map(|&platform| opt(platform.as_ini_str(), platform.to_string(), platform == current))
        .collect()
}

const UTC_OFFSETS: &[&str] = &[
    "UTC-12:00",
    "UTC-11:00",
    "UTC-10:00",
    "UTC-09:30",
    "UTC-09:00",
    "UTC-08:00",
    "UTC-07:00",
    "UTC-06:00",
    "UTC-05:00",
    "UTC-04:00",
    "UTC-03:30",
    "UTC-03:00",
    "UTC-02:00",
    "UTC-01:00",
    "UTC+00:00",
    "UTC+01:00",
    "UTC+02:00",
    "UTC+03:00",
    "UTC+03:30",
    "UTC+04:00",
    "UTC+04:30",
    "UTC+05:00",
    "UTC+05:30",
    "UTC+05:45",
    "UTC+06:00",
    "UTC+06:30",
    "UTC+07:00",
    "UTC+08:00",
    "UTC+08:45",
    "UTC+09:00",
    "UTC+09:30",
    "UTC+10:00",
    "UTC+10:30",
    "UTC+11:00",
    "UTC+12:00",
    "UTC+12:45",
    "UTC+13:00",
    "UTC+14:00",
];

pub fn timezones(current: &str) -> Vec<SelectOption> {
    let current = current.trim();
    let mut options = vec![opt("", "Local time", current.is_empty())];
    options.extend(
        UTC_OFFSETS
            .iter()
            .map(|&offset| opt(offset, offset, offset == current)),
    );
    options
}
