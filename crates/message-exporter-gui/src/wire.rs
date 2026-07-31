//! Slint callback wiring for each tab / chrome control.

use std::sync::{Arc, Mutex};

use chrono::{Datelike, Local, NaiveDate};
use message_exporter_core::VaultSection;
use slint::ComponentHandle;

use crate::browse;
use crate::options;
use crate::start;
use crate::state::AppState;
use crate::sync;
use crate::wsl;
use crate::{
    AppWindow, ContactsAdapter, ConvertAdapter, Date, ExportAdapter, LogAdapter, VaultAdapter,
};

pub fn wire_all(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    wire_about(ui);
    wire_error_dismiss(ui, Arc::clone(&state));
    wire_help(ui, Arc::clone(&state));
    wire_contacts(ui, Arc::clone(&state));
    wire_export(ui, Arc::clone(&state));
    wire_convert(ui, Arc::clone(&state));
    wire_vault(ui, Arc::clone(&state));
    wire_log(ui, Arc::clone(&state));
}

fn wire_about(ui: &AppWindow) {
    let ui_weak = ui.as_weak();
    ui.on_about_toggled(move || {
        if let Some(ui) = ui_weak.upgrade() {
            ui.set_about_open(!ui.get_about_open());
        }
    });
}

fn wire_error_dismiss(ui: &AppWindow, state: Arc<Mutex<AppState>>) {
    let ui_weak = ui.as_weak();
    ui.on_error_dismissed(move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        let mut st = state.lock().expect("state lock");
        st.clear_errors();
        sync::push_chrome(&ui, &st);
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
            start::report_errors(&ui, &mut st, vec![format!("Could not open help: {error}")]);
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
        move || start::start_validate(&ui_weak, &state, true)
    });
    ui.global::<ContactsAdapter>().on_update({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start::start_validate(&ui_weak, &state, false)
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
                start::report_errors(&ui, &mut st, vec![format!("Could not open link: {error}")]);
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
                if let Err(error) = st.save_export_ini() {
                    start::report_errors(&ui, &mut st, vec![error]);
                }
            }
            // Refresh visibility helpers after attachment / platform changes too.
            sync::push_export(&ui, &st);
            sync::push_chrome(&ui, &st);
        }
    });

    ui.global::<ExportAdapter>().on_run({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start::start_export(&ui_weak, &state)
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
            st.clear_errors();
            if let Err(error) = st.save_export_ini() {
                start::report_errors(&ui, &mut st, vec![error]);
            }
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
        move || start::start_reexport(&ui_weak, &state)
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
            st.clear_errors();
            if let Err(error) = st.save_export_ini() {
                start::report_errors(&ui, &mut st, vec![error]);
            }
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
        move || start::start_vault_auth(&ui_weak, &state)
    });
    ui.global::<VaultAdapter>().on_import({
        let ui_weak = ui_weak.clone();
        let state = Arc::clone(&state);
        move || start::start_vault_import(&ui_weak, &state)
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
            st.clear_errors();
            if let Err(error) = st.save_export_ini() {
                start::report_errors(&ui, &mut st, vec![error]);
            }
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
                    sync::append_log_line(&ui, &format!("Could not request cancellation: {error}"));
                    start::report_errors(&ui, &mut st, vec![error]);
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

