use std::sync::Arc;

use axum::Form as AxumForm;
use axum::extract::State;
use axum::response::Redirect;
use message_exporters_core::{AttachmentMedia, ensure_output_dir};
use message_ir::reexport::run as run_reexport;

use crate::jobs::{prepare_library_config, run_and_log};
use crate::options;
use crate::params::{self, Params};
use crate::state::AppState;
use crate::views::{Chrome, ReexportPage};

pub async fn show(State(state): State<Arc<AppState>>) -> ReexportPage {
    let ini = state.ini.lock().expect("ini lock poisoned");
    let form = state.form.lock().expect("form lock poisoned");
    build_page(&ini, &form, state.take_errors())
}

pub async fn run(
    State(state): State<Arc<AppState>>,
    AxumForm(params): AxumForm<Params>,
) -> Redirect {
    let config = {
        let mut ini = state.ini.lock().expect("ini lock poisoned");
        let mut form = state.form.lock().expect("form lock poisoned");

        ini.reexport.input = params::text(&params, "input");
        ini.reexport.output = params::text(&params, "output");
        ini.reexport.output_format =
            params::output_format(&params, "output_format", ini.reexport.output_format);

        form.attachment_media = params::attachment_media(&params, "attachment_media", form.attachment_media);
        if form.attachment_media == AttachmentMedia::Compress {
            form.media_max_resolution =
                params::max_resolution(&params, "media_max_resolution", form.media_max_resolution);
            if let Some(v) = params.get("media_max_fps") {
                form.media_max_fps = v.clone();
            }
            if let Some(v) = params.get("media_min_size") {
                form.media_min_size = v.clone();
            }
            form.media_skip_efficient = params::checkbox(&params, "media_skip_efficient");
        }
        form.obfuscate = params::checkbox(&params, "obfuscate");
        form.obfuscate_seed = params::text(&params, "obfuscate_seed");

        if let Err(error) = ini.save(&form) {
            state.set_errors(vec![error]);
            return Redirect::to("/reexport");
        }

        match form.to_reexport_config(&ini.reexport.input, &ini.reexport.output, ini.reexport.output_format) {
            Ok(config) => config,
            Err(errors) => {
                state.set_errors(errors);
                return Redirect::to("/reexport");
            }
        }
    };

    if let Err(error) = ensure_output_dir(&config.output) {
        state.set_errors(vec![error]);
        return Redirect::to("/reexport");
    }

    let label = "message-reexporter (library)".to_string();
    let job: crate::jobs::LibraryJob = Box::new(move |cancel, tx| {
        let config = prepare_library_config(config, cancel, &tx);
        run_and_log(run_reexport(&config), tx)
    });
    let id = state.jobs.start(label, job);
    Redirect::to(&format!("/jobs/{id}"))
}

pub async fn clear(State(state): State<Arc<AppState>>) -> Redirect {
    let mut ini = state.ini.lock().expect("ini lock poisoned");
    let form = state.form.lock().expect("form lock poisoned");
    ini.reexport = Default::default();
    let _ = ini.save(&form);
    state.set_errors(Vec::new());
    Redirect::to("/reexport")
}

fn build_page(
    ini: &message_exporters_core::ExportIniState,
    form: &message_exporters_core::Form,
    errors: Vec<String>,
) -> ReexportPage {
    let obfuscate_active = form.obfuscate || !form.obfuscate_seed.trim().is_empty();
    let show_ffmpeg_warning =
        !obfuscate_active && form.attachment_media.needs_ffmpeg() && !message_media::ffmpeg_available();
    let show_compress_options = form.attachment_media == AttachmentMedia::Compress;

    ReexportPage {
        chrome: Chrome {
            active_tab: "reexport",
            errors,
        },
        form: form.clone(),
        input: ini.reexport.input.clone(),
        output: ini.reexport.output.clone(),
        ini_path: ini.path.display().to_string(),
        output_format_options: options::output_formats(ini.reexport.output_format),
        attachment_media_options: options::attachment_media(form.attachment_media),
        max_resolution_options: options::max_resolutions(form.media_max_resolution),
        show_ffmpeg_warning,
        show_compress_options,
    }
}
