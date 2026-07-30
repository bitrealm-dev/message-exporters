//! Shared application state: the persisted `export.ini` (same format and,
//! by default, same file the native GUI reads/writes) plus the job registry.

use std::sync::Mutex;

use message_exporters_core::{ExportIniState, Form};

use crate::runner::JobRegistry;

pub struct AppState {
    pub ini: Mutex<ExportIniState>,
    pub form: Mutex<Form>,
    pub jobs: JobRegistry,
    /// Non-fatal errors from the last action (validation failures, load errors),
    /// shown once at the top of the page that produced them.
    pub errors: Mutex<Vec<String>>,
}

impl AppState {
    pub fn new() -> Self {
        let (ini, form, load_error) = ExportIniState::load_or_default();
        Self {
            ini: Mutex::new(ini),
            form: Mutex::new(form),
            jobs: JobRegistry::default(),
            errors: Mutex::new(load_error.into_iter().collect()),
        }
    }

    pub fn take_errors(&self) -> Vec<String> {
        std::mem::take(&mut self.errors.lock().expect("errors lock poisoned"))
    }

    pub fn set_errors(&self, errors: Vec<String>) {
        *self.errors.lock().expect("errors lock poisoned") = errors;
    }
}
