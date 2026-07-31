//! Push `AppState` into Slint adapters and pull adapter values back into `Form`
//! / `ExportIniState` before validation and save.

use message_exporter_core::{
    AttachmentMedia, Exporter, OutputFormat, WhatsappPlatform, contacts_kind_from_path,
};
use media::ffmpeg_available;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;
use vault_push::detect_source as vault_detect_source;

use crate::options;
use crate::state::AppState;
use crate::{AppWindow, ContactsAdapter, ConvertAdapter, ExportAdapter, LogAdapter, VaultAdapter};

pub fn push_static_option_models(ui: &AppWindow) {
    let export = ui.global::<ExportAdapter>();
    export.set_exporter_options(options::exporter_options());
    export.set_exporter_separator_before_index(options::exporter_separator_before_index());
    export.set_attachment_media_options(options::attachment_media_options());
    export.set_max_resolution_options(options::max_resolution_options());
    export.set_apple_platform_options(options::apple_platform_options());
    export.set_whatsapp_platform_options(options::whatsapp_platform_options());
    export.set_timezone_options(options::timezone_options());

    let convert = ui.global::<ConvertAdapter>();
    convert.set_output_format_options(options::output_format_options());
    convert.set_attachment_media_options(options::attachment_media_options());
    convert.set_max_resolution_options(options::max_resolution_options());

    ui.global::<ContactsAdapter>()
        .set_region_options(options::region_options());
}

pub fn push_all(ui: &AppWindow, state: &mut AppState) {
    push_contacts(ui, state);
    push_export(ui, state);
    push_convert(ui, state);
    push_vault(ui, state);
    push_chrome(ui, state);
}

pub fn push_chrome(ui: &AppWindow, state: &AppState) {
    ui.set_error_text(SharedString::from(state.error_text()));
    ui.set_status_text(SharedString::from(state.status_text()));
    ui.set_tabs_enabled(!state.running);
    let export = ui.global::<ExportAdapter>();
    export.set_enabled(!state.running);
    ui.global::<ContactsAdapter>().set_enabled(!state.running);
    ui.global::<ConvertAdapter>().set_enabled(!state.running);
    ui.global::<VaultAdapter>().set_enabled(!state.running);
    ui.global::<LogAdapter>().set_running(state.running);
    ui.global::<LogAdapter>()
        .set_session_log_name(SharedString::from(state.session_log_name()));
    ui.global::<LogAdapter>()
        .set_status_text(SharedString::from(state.status_text()));
}

pub fn push_contacts(ui: &AppWindow, state: &AppState) {
    let contacts = ui.global::<ContactsAdapter>();
    contacts.set_input(SharedString::from(state.validate_input.as_str()));
    contacts.set_region_index(if state.validate_usa { 0 } else { 1 });
}

pub fn pull_contacts(ui: &AppWindow, state: &mut AppState) {
    let contacts = ui.global::<ContactsAdapter>();
    state.validate_input = contacts.get_input().to_string();
    state.validate_usa = contacts.get_region_index() == 0;
}

pub fn push_export(ui: &AppWindow, state: &AppState) {
    let form = &state.form;
    let exporter = state.exporter;
    let export = ui.global::<ExportAdapter>();

    export.set_exporter_key(SharedString::from(exporter.ini_key()));
    export.set_exporter_index(options::exporter_index(exporter));
    export.set_product_link_label(SharedString::from(exporter.link_label()));
    export.set_product_url(SharedString::from(exporter.product_url()));

    export.set_input(SharedString::from(form.input.as_str()));
    export.set_output(SharedString::from(form.output.as_str()));
    export.set_db_path(SharedString::from(form.db_path.as_str()));
    export.set_contacts(SharedString::from(form.contacts.as_str()));
    export.set_owner_phones(SharedString::from(form.owner_phones.as_str()));
    export.set_owner_emails(SharedString::from(form.owner_emails.as_str()));
    export.set_name_mapping(SharedString::from(form.name_mapping.as_str()));
    export.set_timezone_index(options::timezone_index(&form.timezone));

    export.set_attachment_media_index(options::attachment_media_index(form.attachment_media));
    export.set_max_resolution_index(options::max_resolution_index(form.media_max_resolution));
    export.set_media_max_fps(SharedString::from(form.media_max_fps.as_str()));
    export.set_media_min_size(SharedString::from(form.media_min_size.as_str()));
    export.set_media_skip_efficient(form.media_skip_efficient);

    export.set_start_date(SharedString::from(form.start_date.as_str()));
    export.set_end_date(SharedString::from(form.end_date.as_str()));
    export.set_obfuscate(form.obfuscate);
    export.set_obfuscate_seed(SharedString::from(form.obfuscate_seed.as_str()));
    export.set_advanced(form.advanced);

    export.set_whatsapp_platform_index(options::whatsapp_platform_index(form.whatsapp_platform));
    export.set_whatsapp_backup(SharedString::from(form.whatsapp_backup.as_str()));
    export.set_whatsapp_wa(SharedString::from(form.whatsapp_wa.as_str()));
    export.set_whatsapp_key(SharedString::from(form.whatsapp_key.as_str()));
    export.set_whatsapp_media(SharedString::from(form.whatsapp_media.as_str()));
    export.set_whatsapp_db(SharedString::from(form.whatsapp_db.as_str()));
    export.set_whatsapp_business(form.whatsapp_business);

    export.set_apple_platform_index(options::apple_platform_index(form.apple_platform));
    export.set_backup_password(SharedString::from(form.backup_password.as_str()));
    export.set_apple_contacts(SharedString::from(form.apple_contacts.as_str()));
    export.set_attachment_root(SharedString::from(form.attachment_root.as_str()));
    export.set_conversation_filter(SharedString::from(form.conversation_filter.as_str()));

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
    let obfuscate_active = form.obfuscate || !form.obfuscate_seed.trim().is_empty();
    let show_ffmpeg_warning =
        !obfuscate_active && form.attachment_media.needs_ffmpeg() && !ffmpeg_available();
    let show_compress_options = form.attachment_media == AttachmentMedia::Compress;
    let input_label = if exporter == Exporter::SmsBackupPlus {
        "Input file or folder"
    } else {
        "Input directory"
    };

    export.set_is_whatsapp(is_whatsapp);
    export.set_is_imessage(is_imessage);
    export.set_is_imazing(is_imazing);
    export.set_is_sms_backup_plus(is_sms_backup_plus);
    export.set_needs_owner_phones(needs_owner_phones);
    export.set_whatsapp_is_ios(whatsapp_is_ios);
    export.set_show_contacts(show_contacts);
    export.set_show_ffmpeg_warning(show_ffmpeg_warning);
    export.set_show_compress_options(show_compress_options);
    export.set_input_label(SharedString::from(input_label));
}

pub fn pull_export(ui: &AppWindow, state: &mut AppState) {
    let export = ui.global::<ExportAdapter>();
    let form = &mut state.form;
    state.exporter = options::exporter_at(export.get_exporter_index());

    form.output_format = OutputFormat::Jsonl;
    form.input = export.get_input().to_string();
    form.output = export.get_output().to_string();
    form.db_path = export.get_db_path().to_string();
    form.contacts = export.get_contacts().to_string();
    form.contacts_kind = contacts_kind_from_path(&form.contacts);
    form.owner_phones = export.get_owner_phones().to_string();
    form.owner_emails = export.get_owner_emails().to_string();
    form.name_mapping = export.get_name_mapping().to_string();
    form.timezone = options::timezone_at(export.get_timezone_index());

    form.attachment_media = options::attachment_media_at(export.get_attachment_media_index());
    form.media_max_resolution = options::max_resolution_at(export.get_max_resolution_index());
    form.media_max_fps = export.get_media_max_fps().to_string();
    form.media_min_size = export.get_media_min_size().to_string();
    form.media_skip_efficient = export.get_media_skip_efficient();

    form.start_date = export.get_start_date().to_string();
    form.end_date = export.get_end_date().to_string();
    form.obfuscate = export.get_obfuscate();
    form.obfuscate_seed = export.get_obfuscate_seed().to_string();
    form.advanced = export.get_advanced();

    form.whatsapp_platform = options::whatsapp_platform_at(export.get_whatsapp_platform_index());
    form.whatsapp_backup = export.get_whatsapp_backup().to_string();
    form.whatsapp_wa = export.get_whatsapp_wa().to_string();
    form.whatsapp_key = export.get_whatsapp_key().to_string();
    form.whatsapp_media = export.get_whatsapp_media().to_string();
    form.whatsapp_db = export.get_whatsapp_db().to_string();
    form.whatsapp_business = export.get_whatsapp_business();

    form.apple_platform = options::apple_platform_at(export.get_apple_platform_index());
    form.backup_password = export.get_backup_password().to_string();
    form.apple_contacts = export.get_apple_contacts().to_string();
    form.attachment_root = export.get_attachment_root().to_string();
    form.conversation_filter = export.get_conversation_filter().to_string();
}

pub fn push_convert(ui: &AppWindow, state: &AppState) {
    let convert = ui.global::<ConvertAdapter>();
    let form = &state.form;
    convert.set_input(SharedString::from(state.export_ini.reexport.input.as_str()));
    convert.set_output(SharedString::from(
        state.export_ini.reexport.output.as_str(),
    ));
    convert.set_output_format_index(options::output_format_index(
        state.export_ini.reexport.output_format,
    ));
    convert.set_attachment_media_index(options::attachment_media_index(form.attachment_media));
    convert.set_max_resolution_index(options::max_resolution_index(form.media_max_resolution));
    convert.set_media_max_fps(SharedString::from(form.media_max_fps.as_str()));
    convert.set_media_min_size(SharedString::from(form.media_min_size.as_str()));
    convert.set_media_skip_efficient(form.media_skip_efficient);
    convert.set_obfuscate(form.obfuscate);
    convert.set_obfuscate_seed(SharedString::from(form.obfuscate_seed.as_str()));

    let obfuscate_active = form.obfuscate || !form.obfuscate_seed.trim().is_empty();
    convert.set_show_ffmpeg_warning(
        !obfuscate_active && form.attachment_media.needs_ffmpeg() && !ffmpeg_available(),
    );
    convert.set_show_compress_options(form.attachment_media == AttachmentMedia::Compress);
}

pub fn pull_convert(ui: &AppWindow, state: &mut AppState) {
    let convert = ui.global::<ConvertAdapter>();
    state.export_ini.reexport.input = convert.get_input().to_string();
    state.export_ini.reexport.output = convert.get_output().to_string();
    state.export_ini.reexport.output_format =
        options::output_format_at(convert.get_output_format_index());
    state.form.attachment_media =
        options::attachment_media_at(convert.get_attachment_media_index());
    state.form.media_max_resolution =
        options::max_resolution_at(convert.get_max_resolution_index());
    state.form.media_max_fps = convert.get_media_max_fps().to_string();
    state.form.media_min_size = convert.get_media_min_size().to_string();
    state.form.media_skip_efficient = convert.get_media_skip_efficient();
    state.form.obfuscate = convert.get_obfuscate();
    state.form.obfuscate_seed = convert.get_obfuscate_seed().to_string();
}

pub fn push_vault(ui: &AppWindow, state: &mut AppState) {
    state.prefill_vault_input();
    let vault = ui.global::<VaultAdapter>();
    let v = &state.export_ini.vault;
    vault.set_url(SharedString::from(v.url.as_str()));
    vault.set_username(SharedString::from(v.username.as_str()));
    vault.set_key(SharedString::from(v.key.as_str()));
    vault.set_input(SharedString::from(v.input.as_str()));
    vault.set_continue_on_error(v.continue_on_error);
    vault.set_force(v.force);
    vault.set_skip_attachments(v.skip_attachments);

    let note = if !state.vault_source_note.is_empty() {
        state.vault_source_note.clone()
    } else {
        vault_detect_source(std::path::Path::new(v.input.trim()))
            .ok()
            .flatten()
            .map(|s| format!("Detected source: {s}"))
            .unwrap_or_default()
    };
    vault.set_source_note(SharedString::from(note));
}

pub fn pull_vault(ui: &AppWindow, state: &mut AppState) {
    let vault = ui.global::<VaultAdapter>();
    state.export_ini.vault.url = vault.get_url().to_string();
    state.export_ini.vault.username = vault.get_username().to_string();
    state.export_ini.vault.key = vault.get_key().to_string();
    state.export_ini.vault.input = vault.get_input().to_string();
    state.export_ini.vault.continue_on_error = vault.get_continue_on_error();
    state.export_ini.vault.force = vault.get_force();
    state.export_ini.vault.skip_attachments = vault.get_skip_attachments();
}

pub fn set_log_lines(ui: &AppWindow, lines: &[String]) {
    let model = Rc::new(VecModel::from(
        lines
            .iter()
            .map(|l| SharedString::from(l.as_str()))
            .collect::<Vec<_>>(),
    ));
    ui.global::<LogAdapter>()
        .set_lines(ModelRc::from(model.clone()));
}

pub fn append_log_line(ui: &AppWindow, line: &str) {
    let lines = ui.global::<LogAdapter>().get_lines();
    if let Some(model) = lines.as_any().downcast_ref::<VecModel<SharedString>>() {
        model.push(SharedString::from(line));
    } else {
        // First line / model not yet a VecModel — replace.
        set_log_lines(ui, &[line.to_string()]);
    }
}

pub fn clear_log_lines(ui: &AppWindow) {
    set_log_lines(ui, &[]);
}
