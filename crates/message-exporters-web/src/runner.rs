//! Job registry bridging in-process library jobs (std::thread + mpsc, exactly
//! like the native GUI) to HTTP: a job id for redirects, an SSE event stream
//! for live logs, and a cancel endpoint backed by `ProcessControl`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use message_exporters_core::{ProcessControl, ProcessEvent, spawn_job};
use tokio::sync::broadcast;

use crate::jobs::LibraryJob;

static NEXT_JOB_ID: AtomicU64 = AtomicU64::new(1);

pub struct JobHandle {
    pub label: String,
    pub control: ProcessControl,
    pub events: Mutex<Vec<ProcessEvent>>,
    pub sender: broadcast::Sender<ProcessEvent>,
    pub done: std::sync::atomic::AtomicBool,
}

impl JobHandle {
    pub fn is_done(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }

    /// All events seen so far, for rendering a fresh page load without waiting on SSE.
    pub fn events_snapshot(&self) -> Vec<ProcessEvent> {
        self.events.lock().expect("events lock poisoned").clone()
    }
}

#[derive(Default)]
pub struct JobRegistry {
    jobs: Mutex<HashMap<String, Arc<JobHandle>>>,
    latest: Mutex<Option<String>>,
}

impl JobRegistry {
    pub fn get(&self, id: &str) -> Option<Arc<JobHandle>> {
        self.jobs.lock().expect("jobs lock poisoned").get(id).cloned()
    }

    /// Id of the most recently started job, for the top-nav "Log" link.
    pub fn latest_id(&self) -> Option<String> {
        self.latest.lock().expect("latest lock poisoned").clone()
    }

    /// Start a job on a background thread (mirrors the native GUI's `spawn_job`
    /// call in `start_library_job`) and register it under a new id.
    pub fn start(&self, label: String, job: LibraryJob) -> String {
        let id = NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed).to_string();
        let control = ProcessControl::default();
        let (tx, rx) = std::sync::mpsc::channel::<ProcessEvent>();
        let (sender, _receiver) = broadcast::channel(1024);

        let handle = Arc::new(JobHandle {
            label,
            control: control.clone(),
            events: Mutex::new(Vec::new()),
            sender,
            done: std::sync::atomic::AtomicBool::new(false),
        });

        self.jobs
            .lock()
            .expect("jobs lock poisoned")
            .insert(id.clone(), Arc::clone(&handle));
        *self.latest.lock().expect("latest lock poisoned") = Some(id.clone());

        let label_for_job = handle.label.clone();
        spawn_job(control, tx, label_for_job, job);

        // Bridge thread: drain the blocking mpsc receiver into the broadcast
        // channel (for live SSE subscribers) and an in-memory buffer (so a
        // fresh page load / reconnect can replay everything seen so far).
        thread::spawn(move || {
            while let Ok(event) = rx.recv() {
                let finished = matches!(event, ProcessEvent::Finished(_) | ProcessEvent::Error(_));
                handle
                    .events
                    .lock()
                    .expect("events lock poisoned")
                    .push(event.clone());
                let _ = handle.sender.send(event);
                if finished {
                    handle.done.store(true, Ordering::Relaxed);
                    break;
                }
            }
        });

        id
    }
}
