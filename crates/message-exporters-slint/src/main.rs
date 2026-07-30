/*
mod jobs;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use jobs::library_job_for_exporter;
use message_exporters_core::{
    ATTACHMENT_MEDIA, EXPORTERS, OUTPUT_FORMATS_MAIL, ExportIniState, Exporter, Form,
    ProcessControl, ProcessEvent, ensure_output_dir, spawn_job,
};
use slint::{ModelRc, SharedString, VecModel};

slint::include_modules!();

struct AppState {
    export_ini: ExportIniState,
    form: Form,
    exporter: Exporter,
}

fn model(values: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let values = values
        .into_iter()
        .map(SharedString::from)
        .collect::<Vec<_>>();
    Rc::new(VecModel::from(values)).into()
}

fn item_index<T: PartialEq>(items: &[T], value: &T) -> i32 {
    items
        .iter()
        .position(|item| item == value)
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or_default()
}

fn exporter_at(index: i32) -> Exporter {
    usize::try_from(index)
        .ok()
        .and_then(|index| EXPORTERS.get(index))
        .copied()
        .unwrap_or_default()
}

fn sync_window(window: &AppWindow, state: &AppState) {
    window.set_backup_type_index(item_index(&EXPORTERS, &state.exporter));
    window.set_exporter_name(state.exporter.binary().into());
    window.set_output_format_index(item_index(
        &OUTPUT_FORMATS_MAIL,
        &state.form.output_format,
    ));
    window.set_attachment_mode_index(item_index(
        &ATTACHMENT_MEDIA,
        &state.form.attachment_media,
    ));

    let backup_path = match state.exporter {
        Exporter::Imessage => &state.form.db_path,
        Exporter::Whatsapp => &state.form.whatsapp_backup,
        _ => &state.form.input,
    };
    window.set_backup_path(backup_path.as_str().into());
    window.set_output_directory(state.form.output.as_str().into());
    window.set_start_date(state.form.start_date.as_str().into());
    window.set_end_date(state.form.end_date.as_str().into());
    window.set_obfuscate(state.form.obfuscate);
    window.set_advanced_open(state.form.advanced);
    window.set_backup_password(state.form.backup_password.as_str().into());
    window.set_contacts_path(state.form.apple_contacts.as_str().into());
    window.set_attachment_root(state.form.attachment_root.as_str().into());
}

fn pull_window(window: &AppWindow, state: &mut AppState) {
    state.exporter = exporter_at(window.get_backup_type_index());
    state.export_ini.exporter = state.exporter;

    let backup_path = window.get_backup_path().to_string();
    match state.exporter {
        Exporter::Imessage => state.form.db_path = backup_path,
        Exporter::Whatsapp => state.form.whatsapp_backup = backup_path,
        _ => state.form.input = backup_path,
    }

    if let Some(format) = usize::try_from(window.get_output_format_index())
        .ok()
        .and_then(|index| OUTPUT_FORMATS_MAIL.get(index))
    {
        state.form.output_format = *format;
    }
    if let Some(mode) = usize::try_from(window.get_attachment_mode_index())
        .ok()
        .and_then(|index| ATTACHMENT_MEDIA.get(index))
    {
        state.form.attachment_media = *mode;
    }

    state.form.output = window.get_output_directory().to_string();
    state.form.start_date = window.get_start_date().to_string();
    state.form.end_date = window.get_end_date().to_string();
    state.form.obfuscate = window.get_obfuscate();
    state.form.advanced = window.get_advanced_open();
    state.form.backup_password = window.get_backup_password().to_string();
    state.form.apple_contacts = window.get_contacts_path().to_string();
    state.form.attachment_root = window.get_attachment_root().to_string();
}

fn install_folder_picker(
    window: &AppWindow,
    connect: impl FnOnce(Box<dyn Fn()>) + 'static,
    apply: impl Fn(&AppWindow, SharedString) + Copy + 'static,
) {
    let weak = window.as_weak();
    connect(Box::new(move || {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        if let Some(window) = weak.upgrade() {
            apply(&window, path.to_string_lossy().into_owned().into());
        }
    }));
}

fn main() -> Result<(), slint::PlatformError> {
    let (export_ini, form, load_error) = ExportIniState::load_or_default();
    let exporter = export_ini.exporter;
    let state = Rc::new(RefCell::new(AppState {
        export_ini,
        form,
        exporter,
    }));

    let window = AppWindow::new()?;
    window.set_backup_types(model(
        EXPORTERS.into_iter().map(Exporter::dropdown_label),
    ));
    window.set_output_formats(model(
        OUTPUT_FORMATS_MAIL
            .into_iter()
            .map(|format| format.to_string()),
    ));
    window.set_attachment_modes(model(
        ["Clone", "Convert", "Convert & compress", "Do not copy"]
            .into_iter()
            .map(str::to_string),
    ));
    sync_window(&window, &state.borrow());

    let initial_status = load_error.unwrap_or_else(|| {
        format!("Settings: {}", state.borrow().export_ini.path.display())
    });
    window.set_status_text(initial_status.into());

    {
        let weak = window.as_weak();
        let state = Rc::clone(&state);
        window.on_backup_type_changed(move |index| {
            let exporter = exporter_at(index);
            let mut state = state.borrow_mut();
            state
                .export_ini
                .switch_exporter(exporter, &mut state.form);
            state.exporter = exporter;
            state.form.advanced = false;
            if let Some(window) = weak.upgrade() {
                sync_window(&window, &state);
            }
        });
    }

    install_folder_picker(
        &window,
        {
            let window = window.as_weak();
            move |handler| {
                if let Some(window) = window.upgrade() {
                    window.on_browse_backup(handler);
                }
            }
        },
        |window, path| window.set_backup_path(path),
    );
    install_folder_picker(
        &window,
        {
            let window = window.as_weak();
            move |handler| {
                if let Some(window) = window.upgrade() {
                    window.on_browse_output(handler);
                }
            }
        },
        |window, path| window.set_output_directory(path),
    );
    install_folder_picker(
        &window,
        {
            let window = window.as_weak();
            move |handler| {
                if let Some(window) = window.upgrade() {
                    window.on_browse_contacts(handler);
                }
            }
        },
        |window, path| window.set_contacts_path(path),
    );
    install_folder_picker(
        &window,
        {
            let window = window.as_weak();
            move |handler| {
                if let Some(window) = window.upgrade() {
                    window.on_browse_attachments(handler);
                }
            }
        },
        |window, path| window.set_attachment_root(path),
    );

    {
        let weak = window.as_weak();
        let state = Rc::clone(&state);
        window.on_clear(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            pull_window(&window, &mut state);
            state
                .export_ini
                .clear_active_section(&mut state.form);
            let _ = state.export_ini.save(&state.form);
            sync_window(&window, &state);
            window.set_status_text("Fields cleared.".into());
        });
    }

    {
        let weak = window.as_weak();
        let state = Rc::clone(&state);
        window.on_run_exporter(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };

            let (exporter, config) = {
                let mut state = state.borrow_mut();
                pull_window(&window, &mut state);
                if let Err(error) = state.export_ini.save(&state.form) {
                    window.set_status_text(error.into());
                    return;
                }
                let config = match state.form.to_config(state.exporter) {
                    Ok(config) => config,
                    Err(errors) => {
                        window.set_status_text(errors.join(" ").into());
                        return;
                    }
                };
                if let Err(error) = ensure_output_dir(&config.output) {
                    window.set_status_text(error.into());
                    return;
                }
                (state.exporter, config)
            };

            window.set_running(true);
            window.set_status_text(format!("Starting {}…", exporter.binary()).into());

            let (tx, rx) = mpsc::channel();
            spawn_job(
                ProcessControl::default(),
                tx,
                format!("{} (library)", exporter.binary()),
                library_job_for_exporter(exporter, config),
            );

            let weak = weak.clone();
            std::thread::spawn(move || {
                while let Ok(event) = rx.recv() {
                    let (text, done) = match event {
                        ProcessEvent::Started(command) => (format!("Running: {command}"), false),
                        ProcessEvent::Log(line) => (line, false),
                        ProcessEvent::Finished(summary) => (summary, true),
                        ProcessEvent::Error(error) => (format!("Error: {error}"), true),
                    };
                    let weak = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(window) = weak.upgrade() {
                            window.set_status_text(text.into());
                            if done {
                                window.set_running(false);
                            }
                        }
                    });
                    if done {
                        break;
                    }
                }
            });
        });
    }

    window.run()?;

    let mut state = state.borrow_mut();
    pull_window(&window, &mut state);
    let _ = state.export_ini.save(&state.form);
    Ok(())
}
*/
//! Slint desktop GUI for message-exporters.
//!
//! In-process exporter libraries and `export.ini` persistence.

mod browse;
mod jobs;
mod options;
mod session_log;
mod state;
mod sync;
mod wsl;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};

use chrono::{Datelike, Local, NaiveDate};
use jobs::{LibraryJob, library_job_for_exporter, prepare_library_config, run_and_log};
use contacts::{ValidateMode, probe_contacts_input, validate_contacts_file};
use message_exporters_core::{ProcessEvent, VaultSection, spawn_job};
use ir::reexport::run as run_reexport;
use phone::PhoneRegion;
use slint::ComponentHandle;
use state::{AppState, ensure_output_dir_checked};
use vault_push::{
    ProgressEvent as VaultProgressEvent, VaultPushConfig, authenticate as vault_authenticate,
    run as run_vault_push,
};

slint::include_modules!();

const TAB_LOG: i32 = 4;

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    ui.set_app_title(format!("Message Exporters {}", env!("CARGO_PKG_VERSION")).into());
    let state = Arc::new(Mutex::new(AppState::load()));

    sync::push_static_option_models(&ui);
    {
        let mut st = state.lock().expect("state lock");
        sync::push_all(&ui, &mut st);
    }
    sync::clear_log_lines(&ui);

    wire_about(&ui);
    wire_help(&ui, Arc::clone(&state));
    wire_contacts(&ui, Arc::clone(&state));
    wire_export(&ui, Arc::clone(&state));
    wire_convert(&ui, Arc::clone(&state));
    wire_vault(&ui, Arc::clone(&state));
    wire_log(&ui, Arc::clone(&state));

    // Persist when the process exits after `run()` returns.
    let result = ui.run();
    state.lock().expect("state lock").persist_on_exit();
    result
}

fn wire_about(ui: &AppWindow) {
    let ui_weak = ui.as_weak();
    ui.on_about_toggled(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_about_open(!ui.get_about_open());
        }
    });
}

fn wire_help(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    const DOCS_URL: &str = "https://bitrealm-dev.github.io/message-exporters/";

    let ui_weak = ui.as_weak();
    ui.on_help_requested(move || {
        if let Err(error) = wsl::open_url(DOCS_URL)
            && let Some(ui) = ui_weak.upgrade()
        {
            let mut st = state.lock().expect("state lock");
            st.set_errors(vec![format!("Could not open help: {error}")]);
            sync::push_chrome(&ui, &st);
        }
    });
}

fn wire_contacts(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    ui.global::<ContactsAdapter>().on_browse({
        let ui_weak = ui_weak.clone();
        move |field_id| {
            let kind = browse::browse_kind_for_field(&field_id);
            browse::pick_path(ui_weak.clone(), field_id.to_string(), kind);
        }
    });

    ui.global::<ContactsAdapter>().on_check({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start_validate(&ui_weak, &state, true)
    });
    ui.global::<ContactsAdapter>().on_update({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start_validate(&ui_weak, &state, false)
    });
}

fn wire_export(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();

    ui.global::<ExportAdapter>().on_date_for_text(|value| {
        let date = NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
            .unwrap_or_else(|_| Local::now().date_naive());
        Date {
            year: date.year(),
            month: i32::try_from(date.month()).expect("month fits in i32"),
            day: i32::try_from(date.day()).expect("day fits in i32"),
        }
    });

    ui.global::<ExportAdapter>().on_browse({
        let ui_weak = ui_weak.clone();
        move |field_id| {
            let kind = browse::browse_kind_for_field(&field_id);
            browse::pick_path(ui_weak.clone(), field_id.to_string(), kind);
        }
    });

    ui.global::<ExportAdapter>().on_open_product_url({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let url = state
                .lock()
                .expect("state lock")
                .exporter
                .product_url()
                .to_string();
            if let Err(error) = open::that(&url)
                && let Some(ui) = ui_weak.upgrade()
            {
                let mut st = state.lock().expect("state lock");
                st.set_errors(vec![format!("Could not open link: {error}")]);
                sync::push_chrome(&ui, &st);
            }
        }
    });

    ui.global::<ExportAdapter>().on_exporter_changed({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move |index| {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            sync::pull_export(&ui, &mut st);
            let next = options::exporter_at(index);
            if next != st.exporter {
                let AppState {
                    export_ini,
                    form,
                    exporter,
                    ..
                } = &mut *st;
                export_ini.switch_exporter(next, form);
                *exporter = next;
                form.advanced = false;
                st.clear_errors();
                let _ = st.save_export_ini();
            }
            // Refresh visibility helpers after attachment / platform changes too.
            sync::push_export(&ui, &st);
            sync::push_chrome(&ui, &st);
        }
    });

    ui.global::<ExportAdapter>().on_run({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start_export(&ui_weak, &state)
    });
    ui.global::<ExportAdapter>().on_clear({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            sync::pull_export(&ui, &mut st);
            {
                let AppState {
                    export_ini, form, ..
                } = &mut *st;
                export_ini.clear_active_section(form);
            }
            let _ = st.save_export_ini();
            st.clear_errors();
            sync::push_export(&ui, &st);
            sync::push_chrome(&ui, &st);
        }
    });
}

fn wire_convert(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();

    ui.global::<ConvertAdapter>().on_browse({
        let ui_weak = ui_weak.clone();
        move |field_id| {
            let kind = browse::browse_kind_for_field(&field_id);
            browse::pick_path(ui_weak.clone(), field_id.to_string(), kind);
        }
    });

    ui.global::<ConvertAdapter>().on_media_changed({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            sync::pull_convert(&ui, &mut st);
            sync::push_convert(&ui, &st);
        }
    });

    ui.global::<ConvertAdapter>().on_run({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start_reexport(&ui_weak, &state)
    });
    ui.global::<ConvertAdapter>().on_clear({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            st.export_ini.reexport = Default::default();
            let _ = st.save_export_ini();
            st.clear_errors();
            sync::push_convert(&ui, &st);
            sync::push_chrome(&ui, &st);
        }
    });
}

fn wire_vault(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();

    ui.global::<VaultAdapter>().on_browse({
        let ui_weak = ui_weak.clone();
        move |field_id| {
            let kind = browse::browse_kind_for_field(&field_id);
            browse::pick_path(ui_weak.clone(), field_id.to_string(), kind);
        }
    });

    ui.global::<VaultAdapter>().on_authenticate({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start_vault_auth(&ui_weak, &state)
    });
    ui.global::<VaultAdapter>().on_import({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start_vault_import(&ui_weak, &state)
    });
    ui.global::<VaultAdapter>().on_clear({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            st.export_ini.vault = VaultSection {
                continue_on_error: true,
                ..Default::default()
            };
            st.vault_source_note.clear();
            let _ = st.save_export_ini();
            st.clear_errors();
            sync::push_vault(&ui, &mut st);
            sync::push_chrome(&ui, &st);
        }
    });
}

fn wire_log(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    ui.global::<LogAdapter>().on_cancel({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let mut st = state.lock().expect("state lock");
            match st.control.cancel() {
                Ok(()) => {
                    st.append_session_log("Cancellation requested…");
                    sync::append_log_line(&ui, "Cancellation requested…");
                }
                Err(error) => {
                    st.set_errors(vec![error.clone()]);
                    sync::append_log_line(&ui, &format!("Could not request cancellation: {error}"));
                    sync::push_chrome(&ui, &st);
                }
            }
        }
    });
    ui.global::<LogAdapter>().on_clear_view({
        let ui_weak = ui_weak.clone();
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                sync::clear_log_lines(&ui);
            }
        }
    });
}

fn show_errors(ui: &AppWindow, state: &AppState) {
    sync::push_chrome(ui, state);
}

/// Start a library job and bridge its events onto the Slint UI thread.
fn start_library_job(
    ui_weak: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    label: String,
    job: LibraryJob,
) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let (tx, rx) = mpsc::channel::<ProcessEvent>();
    {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        st.begin_session_log();
        st.clear_errors();
        st.running = true;
        st.rx = None;
        spawn_job(st.control.clone(), tx, label, job);
    }
    ui.set_tab_index(TAB_LOG);
    sync::clear_log_lines(&ui);
    sync::push_chrome(&ui, &state.lock().expect("state lock"));

    let ui_weak = ui_weak.clone();
    let state_for_done = Arc::clone(state);
    std::thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            let finished = matches!(event, ProcessEvent::Finished(_) | ProcessEvent::Error(_));
            let is_error = matches!(event, ProcessEvent::Error(_));
            let line = match &event {
                ProcessEvent::Started(s) => format!("$ {s}"),
                ProcessEvent::Log(s) | ProcessEvent::Finished(s) | ProcessEvent::Error(s) => {
                    s.clone()
                }
            };
            let state_clone = Arc::clone(&state_for_done);
            let _ = ui_weak.upgrade_in_event_loop(move |ui| {
                {
                    let st = state_clone.lock().expect("state lock");
                    st.append_session_log(&line);
                }
                sync::append_log_line(&ui, &line);
                if finished {
                    let mut st = state_clone.lock().expect("state lock");
                    st.running = false;
                    if is_error {
                        st.set_errors(vec![line.clone()]);
                    }
                    sync::push_chrome(&ui, &st);
                } else {
                    sync::push_chrome(&ui, &state_clone.lock().expect("state lock"));
                }
            });
            if finished {
                break;
            }
        }
    });
}

fn start_validate(
    ui_weak: &slint::Weak<AppWindow>,
    state: &Arc<Mutex<AppState>>,
    check_only: bool,
) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_contacts(&ui, &mut st);
        let input = st.validate_input.trim();
        if input.is_empty() {
            st.set_errors(vec!["Choose a contacts CSV or VCF file.".into()]);
            show_errors(&ui, &st);
            return;
        }
        let path = PathBuf::from(input);
        if let Err(error) = probe_contacts_input(&path) {
            st.set_errors(vec![error.message]);
            show_errors(&ui, &st);
            return;
        }
        let region = if st.validate_usa {
            PhoneRegion::Usa
        } else {
            PhoneRegion::International
        };
        let mode = if check_only {
            ValidateMode::Check
        } else {
            ValidateMode::Update
        };
        let label = if check_only {
            "contacts-validate --check (library)".to_string()
        } else {
            "contacts-validate (library)".to_string()
        };
        let job: LibraryJob =
            Box::new(
                move |_cancel, tx| match validate_contacts_file(&path, region, mode) {
                    Ok(report) => {
                        for line in report.log_lines {
                            let _ = tx.send(ProcessEvent::Log(line));
                        }
                        Ok(())
                    }
                    Err(error) => Err(format!("{error:#}")),
                },
            );
        drop(st);
        start_library_job(ui_weak, state, label, job);
    }
}

fn start_export(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_export(&ui, &mut st);
        st.export_ini.exporter = st.exporter;
        if let Err(error) = st.save_export_ini() {
            st.set_errors(vec![error]);
            show_errors(&ui, &st);
            return;
        }
        let result = st.form.to_config(st.exporter);
        let config = match result {
            Ok(config) => config,
            Err(errors) => {
                st.set_errors(errors);
                show_errors(&ui, &st);
                return;
            }
        };
        if let Err(error) = ensure_output_dir_checked(&config.output) {
            st.set_errors(vec![error]);
            show_errors(&ui, &st);
            return;
        }
        let label = format!("{} (library)", st.exporter.binary());
        let job = library_job_for_exporter(st.exporter, config);
        Some((label, job))
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job);
    }
}

fn start_reexport(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_convert(&ui, &mut st);
        if let Err(error) = st.save_export_ini() {
            st.set_errors(vec![error]);
            show_errors(&ui, &st);
            return;
        }
        let result = st.form.to_reexport_config(
            &st.export_ini.reexport.input,
            &st.export_ini.reexport.output,
            st.export_ini.reexport.output_format,
        );
        let config = match result {
            Ok(config) => config,
            Err(errors) => {
                st.set_errors(errors);
                show_errors(&ui, &st);
                return;
            }
        };
        if let Err(error) = ensure_output_dir_checked(&config.output) {
            st.set_errors(vec![error]);
            show_errors(&ui, &st);
            return;
        }
        let label = "message-reexporter (library)".to_string();
        let job: LibraryJob = Box::new(move |cancel, tx| {
            let config = prepare_library_config(config, cancel, &tx);
            run_and_log(run_reexport(&config), tx)
        });
        Some((label, job))
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job);
    }
}

fn start_vault_auth(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_vault(&ui, &mut st);
        let url = st.export_ini.vault.url.trim().to_string();
        let username = st.export_ini.vault.username.trim().to_string();
        let key = st.export_ini.vault.key.trim().to_string();
        let mut errors = Vec::new();
        if url.is_empty() {
            errors.push("Vault URL is required.".into());
        }
        if username.is_empty() {
            errors.push("Vault username is required.".into());
        }
        if key.is_empty() {
            errors.push("Vault key is required.".into());
        }
        if !errors.is_empty() {
            st.set_errors(errors);
            show_errors(&ui, &st);
            return;
        }
        if let Err(error) = st.save_export_ini() {
            st.set_errors(vec![error]);
            show_errors(&ui, &st);
            return;
        }
        let label = "vault-push auth".to_string();
        let job: LibraryJob = Box::new(move |_cancel, tx| {
            let _ = tx.send(ProcessEvent::Log(format!(
                "Authenticating {username}@{url}…"
            )));
            match vault_authenticate(&url, &key, &username) {
                Ok(auth) => {
                    let name = auth.username.unwrap_or_else(|| auth.account_id.clone());
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Authenticated as {name} ({})",
                        auth.account_id
                    )));
                    Ok(())
                }
                Err(e) => Err(format!("{e:#}")),
            }
        });
        Some((label, job))
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job);
    }
}

fn start_vault_import(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_vault(&ui, &mut st);
        st.prefill_vault_input();
        let url = st.export_ini.vault.url.trim().to_string();
        let username = st.export_ini.vault.username.trim().to_string();
        let key = st.export_ini.vault.key.trim().to_string();
        let input = st.export_ini.vault.input.trim().to_string();
        let mut errors = Vec::new();
        if url.is_empty() {
            errors.push("Vault URL is required.".into());
        }
        if username.is_empty() {
            errors.push("Vault username is required.".into());
        }
        if key.is_empty() {
            errors.push("Vault key is required.".into());
        }
        if input.is_empty() {
            errors.push("Input directory is required.".into());
        }
        if !errors.is_empty() {
            st.set_errors(errors);
            show_errors(&ui, &st);
            return;
        }
        if let Err(error) = st.save_export_ini() {
            st.set_errors(vec![error]);
            show_errors(&ui, &st);
            return;
        }
        let continue_on_error = st.export_ini.vault.continue_on_error;
        let force = st.export_ini.vault.force;
        let label = "vault-push (library)".to_string();
        let job: LibraryJob = Box::new(move |cancel, tx| {
            let cfg = VaultPushConfig {
                input: PathBuf::from(input),
                base_url: url,
                username,
                key,
                mode: "append".into(),
                continue_on_error,
                force,
                max_retries: 3,
                batch_size: vault_push::DEFAULT_BATCH_SIZE,
                asset_upload_workers: vault_push::DEFAULT_ASSET_UPLOAD_WORKERS,
                report_path: None,
                log_path: None,
                journal_path: None,
                cancel: Some(cancel),
            };
            let mut on_progress = |event: VaultProgressEvent| match event {
                VaultProgressEvent::Log(line) => {
                    let _ = tx.send(ProcessEvent::Log(line));
                }
                VaultProgressEvent::Auth {
                    account_id,
                    username,
                } => {
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Authenticated as {username} ({account_id})"
                    )));
                }
                VaultProgressEvent::FileStart { index, total, file } => {
                    let _ = tx.send(ProcessEvent::Log(format!("File {index}/{total}: {file}")));
                }
                VaultProgressEvent::FileDone { file, status } => {
                    let _ = tx.send(ProcessEvent::Log(format!("{status}: {file}")));
                }
                VaultProgressEvent::Finished(report) => {
                    let _ = tx.send(ProcessEvent::Log(format!(
                        "Import finished ok={} conversations_ok={} failed={} skipped={} messages={}",
                        report.ok,
                        report.conversations_ok,
                        report.conversations_failed,
                        report.conversations_skipped,
                        report.messages
                    )));
                }
            };
            match run_vault_push(&cfg, Some(&mut on_progress)) {
                Ok(report) if report.ok => Ok(()),
                Ok(report) => Err(format!(
                    "import completed with failures (failed={})",
                    report.conversations_failed
                )),
                Err(e) => Err(format!("{e:#}")),
            }
        });
        Some((label, job))
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job);
    }
}
