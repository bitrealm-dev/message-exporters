use std::path::PathBuf;
use std::sync::Arc;

use axum::Form as AxumForm;
use axum::extract::State;
use axum::response::Redirect;
use message_exporters_core::ProcessEvent;
use vault_push::{
    ProgressEvent as VaultProgressEvent, VaultPushConfig, authenticate as vault_authenticate,
    detect_source as vault_detect_source, run as run_vault_push,
};

use crate::jobs::LibraryJob;
use crate::params::{self, Params};
use crate::state::AppState;
use crate::views::{Chrome, VaultPage};

pub async fn show(State(state): State<Arc<AppState>>) -> VaultPage {
    let mut ini = state.ini.lock().expect("ini lock poisoned");
    let form = state.form.lock().expect("form lock poisoned");
    if ini.vault.input.trim().is_empty() {
        let from_export = form.output.trim();
        if !from_export.is_empty() {
            ini.vault.input = from_export.to_string();
        }
    }
    build_page(&ini, state.take_errors())
}

fn build_page(ini: &message_exporters_core::ExportIniState, errors: Vec<String>) -> VaultPage {
    let source_note = vault_detect_source(std::path::Path::new(ini.vault.input.trim()))
        .ok()
        .flatten()
        .map(|source| format!("Detected source: {source}"));
    VaultPage {
        chrome: Chrome {
            active_tab: "vault",
            errors,
        },
        url: ini.vault.url.clone(),
        username: ini.vault.username.clone(),
        key: ini.vault.key.clone(),
        input: ini.vault.input.clone(),
        continue_on_error: ini.vault.continue_on_error,
        force: ini.vault.force,
        source_note,
        ini_path: ini.path.display().to_string(),
    }
}

fn apply_vault_params(ini: &mut message_exporters_core::ExportIniState, params: &Params) {
    ini.vault.url = params::text(params, "url");
    ini.vault.username = params::text(params, "username");
    ini.vault.key = params::text(params, "key");
    ini.vault.input = params::text(params, "input");
    ini.vault.continue_on_error = params::checkbox(params, "continue_on_error");
    ini.vault.force = params::checkbox(params, "force");
}

pub async fn authenticate(
    State(state): State<Arc<AppState>>,
    AxumForm(params): AxumForm<Params>,
) -> Redirect {
    let (url, username, key) = {
        let mut ini = state.ini.lock().expect("ini lock poisoned");
        let form = state.form.lock().expect("form lock poisoned");
        apply_vault_params(&mut ini, &params);
        let mut errors = Vec::new();
        if ini.vault.url.trim().is_empty() {
            errors.push("Vault URL is required.".into());
        }
        if ini.vault.username.trim().is_empty() {
            errors.push("Vault username is required.".into());
        }
        if ini.vault.key.trim().is_empty() {
            errors.push("Vault key is required.".into());
        }
        if !errors.is_empty() {
            state.set_errors(errors);
            return Redirect::to("/vault");
        }
        if let Err(error) = ini.save(&form) {
            state.set_errors(vec![error]);
            return Redirect::to("/vault");
        }
        (ini.vault.url.trim().to_string(), ini.vault.username.trim().to_string(), ini.vault.key.trim().to_string())
    };

    let label = "vault-push auth".to_string();
    let job: LibraryJob = Box::new(move |_cancel, tx| {
        let _ = tx.send(ProcessEvent::Log(format!("Authenticating {username}@{url}…")));
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
    let id = state.jobs.start(label, job);
    Redirect::to(&format!("/jobs/{id}"))
}

pub async fn import(
    State(state): State<Arc<AppState>>,
    AxumForm(params): AxumForm<Params>,
) -> Redirect {
    let (url, username, key, input, continue_on_error, force) = {
        let mut ini = state.ini.lock().expect("ini lock poisoned");
        let form = state.form.lock().expect("form lock poisoned");
        apply_vault_params(&mut ini, &params);
        let mut errors = Vec::new();
        if ini.vault.url.trim().is_empty() {
            errors.push("Vault URL is required.".into());
        }
        if ini.vault.username.trim().is_empty() {
            errors.push("Vault username is required.".into());
        }
        if ini.vault.key.trim().is_empty() {
            errors.push("Vault key is required.".into());
        }
        if ini.vault.input.trim().is_empty() {
            errors.push("Input directory is required.".into());
        }
        if !errors.is_empty() {
            state.set_errors(errors);
            return Redirect::to("/vault");
        }
        if let Err(error) = ini.save(&form) {
            state.set_errors(vec![error]);
            return Redirect::to("/vault");
        }
        (
            ini.vault.url.trim().to_string(),
            ini.vault.username.trim().to_string(),
            ini.vault.key.trim().to_string(),
            ini.vault.input.trim().to_string(),
            ini.vault.continue_on_error,
            ini.vault.force,
        )
    };

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
            VaultProgressEvent::Auth { account_id, username } => {
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
    let id = state.jobs.start(label, job);
    Redirect::to(&format!("/jobs/{id}"))
}

pub async fn clear(State(state): State<Arc<AppState>>) -> Redirect {
    let mut ini = state.ini.lock().expect("ini lock poisoned");
    let form = state.form.lock().expect("form lock poisoned");
    ini.vault = message_exporters_core::VaultSection {
        continue_on_error: true,
        ..Default::default()
    };
    let _ = ini.save(&form);
    state.set_errors(Vec::new());
    Redirect::to("/vault")
}
