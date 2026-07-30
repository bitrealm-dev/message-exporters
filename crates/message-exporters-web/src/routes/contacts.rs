use std::path::PathBuf;
use std::sync::Arc;

use axum::Form as AxumForm;
use axum::extract::State;
use axum::response::Redirect;
use message_contacts::{ValidateMode, probe_contacts_input, validate_contacts_file};
use message_exporters_core::ProcessEvent;
use message_phone::PhoneRegion;

use crate::params::Params;
use crate::state::AppState;
use crate::views::{Chrome, ContactsPage};

pub async fn show(State(state): State<Arc<AppState>>) -> ContactsPage {
    ContactsPage {
        chrome: Chrome {
            active_tab: "contacts",
            errors: state.take_errors(),
        },
        input: String::new(),
        usa: true,
    }
}

pub async fn submit(
    State(state): State<Arc<AppState>>,
    AxumForm(params): AxumForm<Params>,
) -> Redirect {
    let input = params.get("input").cloned().unwrap_or_default();
    let usa = params.get("region").map(String::as_str) != Some("international");
    let check_only = params.get("action").map(String::as_str) == Some("check");

    let trimmed = input.trim();
    if trimmed.is_empty() {
        state.set_errors(vec!["Choose a contacts CSV or VCF file.".into()]);
        return Redirect::to("/contacts");
    }
    let path = PathBuf::from(trimmed);
    if let Err(error) = probe_contacts_input(&path) {
        state.set_errors(vec![error.message]);
        return Redirect::to("/contacts");
    }

    let region = if usa { PhoneRegion::Usa } else { PhoneRegion::International };
    let mode = if check_only { ValidateMode::Check } else { ValidateMode::Update };
    let label = if check_only {
        "contacts-validate --check (library)".to_string()
    } else {
        "contacts-validate (library)".to_string()
    };

    let job: crate::jobs::LibraryJob = Box::new(move |_cancel, tx| match validate_contacts_file(&path, region, mode) {
        Ok(report) => {
            for line in report.log_lines {
                let _ = tx.send(ProcessEvent::Log(line));
            }
            Ok(())
        }
        Err(error) => Err(format!("{error:#}")),
    });

    let id = state.jobs.start(label, job);
    Redirect::to(&format!("/jobs/{id}"))
}
