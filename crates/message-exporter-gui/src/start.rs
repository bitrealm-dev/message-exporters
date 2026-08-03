//! Job start helpers and the shared library-job event bridge.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};

use contacts::{ValidateMode, probe_contacts_input, validate_contacts_file};
use message_reexport::run as run_format;
use message_exporter_core::{ProcessEvent, ensure_output_dir, spawn_job};
use phone::PhoneRegion;
use vault_push::{
    ProgressEvent as VaultProgressEvent, VaultPushConfig, authenticate as vault_authenticate,
    run as run_vault_push,
};

use crate::jobs::{LibraryJob, library_job_for_exporter, prepare_library_config, run_and_log};
use crate::state::AppState;
use crate::sync;
use crate::AppWindow;

const TAB_LOG: i32 = 4;

pub(crate) fn report_errors(ui: &AppWindow, state: &mut AppState, errors: Vec<String>) {
    state.set_errors(errors, ui.get_tab_index());
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
    let source_tab = ui.get_tab_index();
    let (tx, rx) = mpsc::channel::<ProcessEvent>();
    {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        st.begin_session_log();
        st.clear_errors();
        st.running = true;
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
                        st.set_errors(vec![line.clone()], source_tab);
                    }
                    sync::push_chrome(&ui, &st);
                }
            });
            if finished {
                break;
            }
        }
    });
}

pub(crate) fn start_validate(
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
            report_errors(
                &ui,
                &mut st,
                vec!["Choose a contacts CSV or VCF file.".into()],
            );
            return;
        }
        let path = PathBuf::from(input);
        if let Err(error) = probe_contacts_input(&path) {
            report_errors(&ui, &mut st, vec![error.message]);
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

pub(crate) fn start_extract(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_extract(&ui, &mut st);
        st.export_ini.exporter = st.exporter;
        if let Err(error) = st.save_export_ini() {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }
        let result = st.form.to_config(st.exporter);
        let config = match result {
            Ok(config) => config,
            Err(errors) => {
                report_errors(&ui, &mut st, errors);
                return;
            }
        };
        if let Err(error) = ensure_output_dir(&config.output) {
            report_errors(&ui, &mut st, vec![error]);
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

pub(crate) fn start_format(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
    let Some(ui) = ui_weak.upgrade() else {
        return;
    };
    let job_and_label = {
        let mut st = state.lock().expect("state lock");
        if st.running {
            return;
        }
        sync::pull_format(&ui, &mut st);
        if let Err(error) = st.save_export_ini() {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }
        let result = st.form.to_format_config(
            &st.export_ini.format.input,
            &st.export_ini.format.output,
            st.export_ini.format.output_format,
        );
        let config = match result {
            Ok(config) => config,
            Err(errors) => {
                report_errors(&ui, &mut st, errors);
                return;
            }
        };
        if let Err(error) = ensure_output_dir(&config.output) {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }
        let label = "Format (library)".to_string();
        let job: LibraryJob = Box::new(move |cancel, tx| {
            let config = prepare_library_config(config, cancel, &tx);
            run_and_log(run_format(&config), tx)
        });
        Some((label, job))
    };
    if let Some((label, job)) = job_and_label {
        start_library_job(ui_weak, state, label, job);
    }
}

pub(crate) fn start_vault_auth(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
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
        let key = st.export_ini.vault.key.trim().to_string();
        let mut errors = Vec::new();
        if url.is_empty() {
            errors.push("Vault URL is required.".into());
        }
        if key.is_empty() {
            errors.push("Vault key is required.".into());
        }
        if !errors.is_empty() {
            report_errors(&ui, &mut st, errors);
            return;
        }
        if let Err(error) = st.save_export_ini() {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }
        let label = "vault-push auth".to_string();
        let job: LibraryJob = Box::new(move |_cancel, tx| {
            let _ = tx.send(ProcessEvent::Log(format!("Authenticating {url}…")));
            match vault_authenticate(&url, &key, "") {
                Ok(auth) => {
                    let name = auth
                        .username
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| auth.account_id.clone());
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

pub(crate) fn start_vault_import(ui_weak: &slint::Weak<AppWindow>, state: &Arc<Mutex<AppState>>) {
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
        let key = st.export_ini.vault.key.trim().to_string();
        let input = st.export_ini.vault.input.trim().to_string();
        let mut errors = Vec::new();
        if url.is_empty() {
            errors.push("Vault URL is required.".into());
        }
        if key.is_empty() {
            errors.push("Vault key is required.".into());
        }
        if input.is_empty() {
            errors.push("Input directory is required.".into());
        }
        if !errors.is_empty() {
            report_errors(&ui, &mut st, errors);
            return;
        }
        if let Err(error) = st.save_export_ini() {
            report_errors(&ui, &mut st, vec![error]);
            return;
        }
        let continue_on_error = st.export_ini.vault.continue_on_error;
        let force = st.export_ini.vault.force;
        let skip_attachments = st.export_ini.vault.skip_attachments;
        let label = "vault-push (library)".to_string();
        let job: LibraryJob = Box::new(move |cancel, tx| {
            let cfg = VaultPushConfig {
                input: PathBuf::from(input),
                base_url: url,
                username: String::new(),
                key,
                mode: "append".into(),
                continue_on_error,
                force,
                skip_attachments,
                verify_digests: false,
                max_retries: 3,
                batch_size: vault_push::DEFAULT_BATCH_SIZE,
                asset_upload_workers: vault_push::DEFAULT_ASSET_UPLOAD_WORKERS,
                asset_multipart_threshold: vault_push::MAX_PROXY_BODY_BYTES,
                asset_max_bytes: vault_push::DEFAULT_ASSET_MAX_BYTES,
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
                        "Import finished ok={} conversations_ok={} failed={} skipped={} messages={} \
                         elapsed_ms={} ({})",
                        report.ok,
                        report.conversations_ok,
                        report.conversations_failed,
                        report.conversations_skipped,
                        report.messages,
                        report.elapsed_ms,
                        vault_push::format_duration_ms(report.elapsed_ms)
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
