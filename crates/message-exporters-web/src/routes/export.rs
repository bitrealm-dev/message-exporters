use std::sync::Arc;

use axum::extract::{Query, State};
use axum::response::Redirect;
use axum::Form as AxumForm;
use message_exporters_core::{
    AttachmentMedia, Exporter, WhatsappPlatform, ensure_output_dir,
};

use crate::jobs::library_job_for_exporter;
use crate::options;
use crate::params::{self, Params};
use crate::state::AppState;
use crate::views::{Chrome, ExportPage};

#[derive(serde::Deserialize)]
pub struct ExportQuery {
    #[serde(default)]
    exporter: Option<String>,
}

pub async fn show(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ExportQuery>,
) -> ExportPage {
    let mut ini = state.ini.lock().expect("ini lock poisoned");
    let mut form = state.form.lock().expect("form lock poisoned");

    if let Some(requested) = query.exporter.as_deref().and_then(Exporter::from_ini_key)
        && requested != ini.exporter
    {
        ini.switch_exporter(requested, &mut form);
        ini.exporter = requested;
        let _ = ini.save(&form);
    }

    build_page(&ini, &form, state.take_errors())
}

pub async fn run(
    State(state): State<Arc<AppState>>,
    AxumForm(params): AxumForm<Params>,
) -> Redirect {
    let Some(exporter) = params::exporter(&params, "exporter") else {
        state.set_errors(vec!["Unknown backup type.".into()]);
        return Redirect::to("/export");
    };

    let config = {
        let mut ini = state.ini.lock().expect("ini lock poisoned");
        let mut form = state.form.lock().expect("form lock poisoned");
        if ini.exporter != exporter {
            ini.switch_exporter(exporter, &mut form);
        }
        ini.exporter = exporter;
        apply_export_params(&mut form, &params, exporter);
        ini.capture_form_section(&form);
        if let Err(error) = ini.save(&form) {
            state.set_errors(vec![error]);
            return Redirect::to("/export");
        }

        match form.to_config(exporter) {
            Ok(config) => config,
            Err(errors) => {
                state.set_errors(errors);
                return Redirect::to("/export");
            }
        }
    };

    if let Err(error) = ensure_output_dir(&config.output) {
        state.set_errors(vec![error]);
        return Redirect::to("/export");
    }

    let label = format!("{} (library)", exporter.binary());
    let job = library_job_for_exporter(exporter, config);
    let id = state.jobs.start(label, job);
    Redirect::to(&format!("/jobs/{id}"))
}

pub async fn clear(State(state): State<Arc<AppState>>) -> Redirect {
    let mut ini = state.ini.lock().expect("ini lock poisoned");
    let mut form = state.form.lock().expect("form lock poisoned");
    ini.clear_active_section(&mut form);
    let _ = ini.save(&form);
    state.set_errors(Vec::new());
    Redirect::to("/export")
}

fn apply_export_params(form: &mut message_exporters_core::Form, params: &Params, exporter: Exporter) {
    form.output_format = params::output_format(params, "output_format", form.output_format);
    form.attachment_media = params::attachment_media(params, "attachment_media", form.attachment_media);
    if form.attachment_media == AttachmentMedia::Compress {
        form.media_max_resolution =
            params::max_resolution(params, "media_max_resolution", form.media_max_resolution);
        if let Some(v) = params.get("media_max_fps") {
            form.media_max_fps = v.clone();
        }
        if let Some(v) = params.get("media_min_size") {
            form.media_min_size = v.clone();
        }
        form.media_skip_efficient = params::checkbox(params, "media_skip_efficient");
    }

    form.start_date = params::text(params, "start_date");
    form.end_date = params::text(params, "end_date");
    form.obfuscate = params::checkbox(params, "obfuscate");
    form.obfuscate_seed = params::text(params, "obfuscate_seed");

    if exporter == Exporter::Whatsapp {
        form.whatsapp_platform =
            params::whatsapp_platform(params, "whatsapp_platform", form.whatsapp_platform);
        form.whatsapp_backup = params::text(params, "whatsapp_backup");
        form.whatsapp_wa = params::text(params, "whatsapp_wa");
        form.output = params::text(params, "output");
        if form.whatsapp_platform == WhatsappPlatform::Android {
            form.whatsapp_key = params::text(params, "whatsapp_key");
            form.whatsapp_media = params::text(params, "whatsapp_media");
            form.whatsapp_db = params::text(params, "whatsapp_db");
        }
        form.whatsapp_business = params::checkbox(params, "whatsapp_business");
        return;
    }

    if exporter == Exporter::Imessage {
        form.db_path = params::text(params, "db_path");
    } else {
        form.input = params::text(params, "input");
    }
    form.output = params::text(params, "output");

    if exporter != Exporter::Imessage {
        form.contacts = params::text(params, "contacts");
        form.contacts_kind = params::refresh_contacts_kind(&form.contacts);
    }

    match exporter {
        Exporter::GoSmsPro | Exporter::SmsBackupRestore => {
            form.owner_phones = params::text(params, "owner_phones");
        }
        Exporter::SmsBackupPlus => {
            form.owner_phones = params::text(params, "owner_phones");
            form.owner_emails = params::text(params, "owner_emails");
            form.name_mapping = params::text(params, "name_mapping");
        }
        Exporter::Imazing => {
            form.timezone = params::text(params, "timezone");
        }
        Exporter::Imessage => {
            form.apple_platform = params::apple_platform(params, "apple_platform", form.apple_platform);
            form.backup_password = params::text(params, "backup_password");
            form.apple_contacts = params::text(params, "apple_contacts");
            form.attachment_root = params::text(params, "attachment_root");
            form.conversation_filter = params::text(params, "conversation_filter");
        }
        Exporter::OpenExtract | Exporter::Whatsapp => {}
    }
}

fn build_page(
    ini: &message_exporters_core::ExportIniState,
    form: &message_exporters_core::Form,
    errors: Vec<String>,
) -> ExportPage {
    let exporter = ini.exporter;
    let form = form.clone();

    let is_whatsapp = exporter == Exporter::Whatsapp;
    let is_imessage = exporter == Exporter::Imessage;
    let is_imazing = exporter == Exporter::Imazing;
    let is_sms_backup_plus = exporter == Exporter::SmsBackupPlus;
    let needs_owner_phones = matches!(
        exporter,
        Exporter::GoSmsPro | Exporter::SmsBackupRestore | Exporter::SmsBackupPlus
    );
    let whatsapp_is_ios = form.whatsapp_platform == WhatsappPlatform::Ios;
    let show_contacts = !is_imessage && !is_whatsapp;

    let input_label = if exporter == Exporter::SmsBackupPlus {
        "Input file or folder"
    } else {
        "Input directory"
    };

    let obfuscate_active = form.obfuscate || !form.obfuscate_seed.trim().is_empty();
    let show_ffmpeg_warning = !obfuscate_active
        && form.attachment_media.needs_ffmpeg()
        && !message_media::ffmpeg_available();
    let show_compress_options = form.attachment_media == AttachmentMedia::Compress;

    ExportPage {
        chrome: Chrome {
            active_tab: "export",
            errors,
        },
        exporter_options: options::exporters(exporter),
        output_format_options: options::output_formats(form.output_format),
        attachment_media_options: options::attachment_media(form.attachment_media),
        max_resolution_options: options::max_resolutions(form.media_max_resolution),
        apple_platform_options: options::apple_platforms(form.apple_platform),
        whatsapp_platform_options: options::whatsapp_platforms(form.whatsapp_platform),
        timezone_options: options::timezones(&form.timezone),
        ini_path: ini.path.display().to_string(),
        exporter,
        form,
        is_whatsapp,
        is_imessage,
        is_imazing,
        is_sms_backup_plus,
        needs_owner_phones,
        whatsapp_is_ios,
        show_contacts,
        input_label,
        show_ffmpeg_warning,
        show_compress_options,
    }
}
