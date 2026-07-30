//! Job log page: an initial buffered render (works with JS disabled, via
//! `<noscript>`) plus a live SSE stream consumed by `assets/app.js`.

use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Redirect, Response};
use futures_util::StreamExt;
use futures_util::stream::{self, Stream};
use message_exporters_core::ProcessEvent;
use tokio_stream::wrappers::BroadcastStream;

use crate::state::AppState;
use crate::views::{Chrome, JobPage};

pub async fn show(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let Some(handle) = state.jobs.get(&id) else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            "Unknown job id (the server may have restarted since it ran).",
        )
            .into_response();
    };

    let buffered_lines = handle.events_snapshot().iter().map(event_text).collect();
    JobPage {
        chrome: Chrome {
            active_tab: "log",
            errors: Vec::new(),
        },
        job_id: id,
        label: handle.label.clone(),
        done: handle.is_done(),
        buffered_lines,
    }
    .into_response()
}

/// `/jobs/latest`: nav "Log" link target when no job id is known yet.
pub async fn latest(State(state): State<Arc<AppState>>) -> Redirect {
    match state.jobs.latest_id() {
        Some(id) => Redirect::to(&format!("/jobs/{id}")),
        None => Redirect::to("/export"),
    }
}

pub async fn cancel(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Redirect {
    if let Some(handle) = state.jobs.get(&id) {
        let _ = handle.control.cancel();
    }
    Redirect::to(&format!("/jobs/{id}"))
}

pub async fn events(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let handle = state.jobs.get(&id);
    let (buffered, receiver) = match &handle {
        Some(handle) => (handle.events_snapshot(), Some(handle.sender.subscribe())),
        None => (Vec::new(), None),
    };

    let replay = stream::iter(buffered.into_iter().map(|event| Ok(to_sse_event(&event))));
    let boxed_replay: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> =
        Box::pin(replay);

    let boxed_live: Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>> = match receiver
    {
        Some(rx) => Box::pin(
            BroadcastStream::new(rx)
                .filter_map(|item| async move { item.ok() })
                .map(|event| Ok(to_sse_event(&event))),
        ),
        None => Box::pin(stream::empty()),
    };

    Sse::new(boxed_replay.chain(boxed_live))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

fn to_sse_event(event: &ProcessEvent) -> Event {
    let (name, data) = match event {
        ProcessEvent::Started(line) => ("started", line.clone()),
        ProcessEvent::Log(line) => ("log", line.clone()),
        ProcessEvent::Finished(line) => ("finished", line.clone()),
        ProcessEvent::Error(line) => ("error-event", line.clone()),
    };
    Event::default().event(name).data(data)
}

fn event_text(event: &ProcessEvent) -> String {
    match event {
        ProcessEvent::Started(line) => format!("$ {line}"),
        ProcessEvent::Log(line) | ProcessEvent::Finished(line) | ProcessEvent::Error(line) => {
            line.clone()
        }
    }
}
