//! Askama template structs. Kept free of business logic: routes build these
//! from `AppState` + request params, then hand them to askama for rendering.

use askama::Template;
use askama_web::WebTemplate;
use message_exporters_core::{Exporter, Form};

use crate::options::SelectOption;

/// Fields every page needs: active tab (for nav highlighting) and any
/// one-shot errors from the last action (validation failures, save errors).
#[derive(Debug, Clone, Default)]
pub struct Chrome {
    pub active_tab: &'static str,
    pub errors: Vec<String>,
}

#[derive(Template, WebTemplate)]
#[template(path = "contacts.html")]
pub struct ContactsPage {
    pub chrome: Chrome,
    pub input: String,
    pub usa: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "export.html")]
pub struct ExportPage {
    pub chrome: Chrome,
    pub form: Form,
    pub exporter: Exporter,
    pub ini_path: String,

    pub exporter_options: Vec<SelectOption>,
    pub output_format_options: Vec<SelectOption>,
    pub attachment_media_options: Vec<SelectOption>,
    pub max_resolution_options: Vec<SelectOption>,
    pub apple_platform_options: Vec<SelectOption>,
    pub whatsapp_platform_options: Vec<SelectOption>,
    pub timezone_options: Vec<SelectOption>,

    pub is_whatsapp: bool,
    pub is_imessage: bool,
    pub is_imazing: bool,
    pub is_sms_backup_plus: bool,
    pub needs_owner_phones: bool,
    pub whatsapp_is_ios: bool,
    pub show_contacts: bool,
    pub input_label: &'static str,
    pub show_ffmpeg_warning: bool,
    pub show_compress_options: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "reexport.html")]
pub struct ReexportPage {
    pub chrome: Chrome,
    pub form: Form,
    pub input: String,
    pub output: String,
    pub ini_path: String,
    pub output_format_options: Vec<SelectOption>,
    pub attachment_media_options: Vec<SelectOption>,
    pub max_resolution_options: Vec<SelectOption>,
    pub show_ffmpeg_warning: bool,
    pub show_compress_options: bool,
}

#[derive(Template, WebTemplate)]
#[template(path = "vault.html")]
pub struct VaultPage {
    pub chrome: Chrome,
    pub url: String,
    pub username: String,
    pub key: String,
    pub input: String,
    pub continue_on_error: bool,
    pub force: bool,
    pub source_note: Option<String>,
    pub ini_path: String,
}

#[derive(Template, WebTemplate)]
#[template(path = "job.html")]
pub struct JobPage {
    pub chrome: Chrome,
    pub job_id: String,
    pub label: String,
    pub done: bool,
    pub buffered_lines: Vec<String>,
}
