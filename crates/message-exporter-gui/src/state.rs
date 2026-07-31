//! Application state: the same `ExportIniState` + `Form` the other GUIs use,
//! plus job control and the session log.

use message_exporter_core::{ExportIniState, Exporter, Form, ProcessControl};

use crate::session_log::SessionLog;

pub struct AppState {
    pub export_ini: ExportIniState,
    pub form: Form,
    pub exporter: Exporter,
    pub auto_save_export_ini: bool,
    pub validate_input: String,
    pub validate_usa: bool,
    pub running: bool,
    pub control: ProcessControl,
    pub session_log: Option<SessionLog>,
    pub errors: Vec<String>,
    /// Tab index that produced `errors` (banner also shows on Log).
    pub error_source_tab: Option<i32>,
    pub vault_source_note: String,
}

impl AppState {
    pub fn load() -> Self {
        let (export_ini, form, load_error) = ExportIniState::load_or_default();
        let exporter = export_ini.exporter;
        let error_source_tab = load_error.as_ref().map(|_| 0);
        Self {
            export_ini,
            form,
            exporter,
            auto_save_export_ini: load_error.is_none(),
            validate_input: String::new(),
            validate_usa: true,
            running: false,
            control: ProcessControl::default(),
            session_log: None,
            errors: load_error.into_iter().collect(),
            error_source_tab,
            vault_source_note: String::new(),
        }
    }

    pub fn save_export_ini(&mut self) -> Result<(), String> {
        if !self.auto_save_export_ini {
            return Ok(());
        }
        self.export_ini.exporter = self.exporter;
        self.export_ini.save(&self.form)
    }

    pub fn persist_on_exit(&mut self) {
        if let Err(error) = self.save_export_ini() {
            eprintln!("Could not save settings: {error}");
        }
    }

    pub fn set_errors(&mut self, errors: Vec<String>, source_tab: i32) {
        self.errors = errors;
        self.error_source_tab = Some(source_tab);
    }

    pub fn clear_errors(&mut self) {
        self.errors.clear();
        self.error_source_tab = None;
    }

    pub fn error_text(&self) -> String {
        self.errors.join("\n")
    }

    pub fn prefill_vault_input(&mut self) {
        if self.export_ini.vault.input.trim().is_empty() {
            let from_export = self.form.output.trim();
            if !from_export.is_empty() {
                self.export_ini.vault.input = from_export.to_string();
            }
        }
    }

    pub fn begin_session_log(&mut self) {
        if self.session_log.is_none() {
            self.session_log = Some(SessionLog::new());
        } else if let Some(log) = &self.session_log {
            log.truncate();
        }
    }

    pub fn append_session_log(&self, line: &str) {
        if let Some(log) = &self.session_log {
            log.append(line);
        }
    }

    pub fn session_log_name(&self) -> String {
        self.session_log
            .as_ref()
            .map(|l| l.name.clone())
            .unwrap_or_default()
    }

    pub fn status_text(&self) -> String {
        if self.running {
            "Running…".into()
        } else {
            format!("Settings: {}", self.export_ini.path.display())
        }
    }
}
