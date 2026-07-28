//! Spawn exporter / contacts-validate CLIs and stream output via mpsc.
//! Also runs in-process jobs (e.g. GO SMS Pro library) on a background thread.

use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

/// Shared cancel flag for cooperative in-process jobs.
pub type CancelFlag = Arc<AtomicBool>;

#[derive(Debug, Clone)]
pub enum ProcessEvent {
    Started(String),
    Log(String),
    Finished(String),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct ProcessControl {
    child: Arc<Mutex<Option<Child>>>,
    cancel: CancelFlag,
}

impl Default for ProcessControl {
    fn default() -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ProcessControl {
    /// Shared cancel flag for in-process jobs. Reset when a new job starts.
    pub fn cancel_flag(&self) -> CancelFlag {
        Arc::clone(&self.cancel)
    }

    pub fn cancel(&self) -> Result<(), String> {
        self.cancel.store(true, Ordering::Relaxed);
        let mut guard = self.child.lock().map_err(|_| "process lock poisoned")?;
        if let Some(child) = guard.as_mut() {
            child
                .kill()
                .map_err(|error| format!("Could not cancel process: {error}"))?;
            return Ok(());
        }
        // In-process job: flag is enough.
        Ok(())
    }

    fn clear(&self) {
        if let Ok(mut child) = self.child.lock() {
            *child = None;
        }
    }

    fn begin_job(&self) {
        self.cancel.store(false, Ordering::Relaxed);
    }
}

pub fn resolve_binary(name: &str) -> Result<PathBuf, String> {
    let executable = executable_name(name);
    let mut tried = Vec::new();

    if let Ok(current) = std::env::current_exe()
        && let Some(dir) = current.parent()
    {
        let sibling = dir.join(&executable);
        tried.push(sibling.clone());
        if sibling.is_file() {
            return Ok(sibling);
        }
    }

    if let Some(extra) = std::env::var_os("MESSAGE_EXPORTERS_BIN") {
        let candidate = PathBuf::from(extra).join(&executable);
        tried.push(candidate.clone());
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            let candidate = directory.join(&executable);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    Err(format!(
        "Could not find {executable}. Build the tool in the same profile as the GUI, \
         add it to PATH, or set MESSAGE_EXPORTERS_BIN. Tried: {}",
        tried
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// Spawn `program` with `args` on a background thread; send events on `tx`.
pub fn spawn(
    program: PathBuf,
    args: Vec<OsString>,
    control: ProcessControl,
    tx: mpsc::Sender<ProcessEvent>,
) {
    control.begin_job();
    thread::spawn(move || {
        let command_display = format_command(&program, &args);
        let mut command = Command::new(&program);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = tx.send(ProcessEvent::Error(format!(
                    "Could not start {}: {error}",
                    program.display()
                )));
                return;
            }
        };
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        if let Ok(mut slot) = control.child.lock() {
            *slot = Some(child);
        } else {
            let _ = tx.send(ProcessEvent::Error("Process lock poisoned.".into()));
            return;
        }

        let _ = tx.send(ProcessEvent::Started(command_display));

        let tx_out = tx.clone();
        let t_out = thread::spawn(move || {
            if let Some(out) = stdout {
                for line in BufReader::new(out).lines() {
                    match line {
                        Ok(line) => {
                            let _ = tx_out.send(ProcessEvent::Log(line));
                        }
                        Err(error) => {
                            let _ = tx_out.send(ProcessEvent::Log(format!(
                                "stdout read error: {error}"
                            )));
                            break;
                        }
                    }
                }
            }
        });
        let tx_err = tx.clone();
        let t_err = thread::spawn(move || {
            if let Some(err) = stderr {
                for line in BufReader::new(err).lines() {
                    match line {
                        Ok(line) => {
                            let _ = tx_err.send(ProcessEvent::Log(line));
                        }
                        Err(error) => {
                            let _ = tx_err.send(ProcessEvent::Log(format!(
                                "stderr read error: {error}"
                            )));
                            break;
                        }
                    }
                }
            }
        });
        let _ = t_out.join();
        let _ = t_err.join();

        let status = {
            let mut guard = match control.child.lock() {
                Ok(g) => g,
                Err(_) => {
                    let _ = tx.send(ProcessEvent::Error("Process lock poisoned.".into()));
                    return;
                }
            };
            match guard.as_mut() {
                Some(child) => child.wait(),
                None => {
                    let _ = tx.send(ProcessEvent::Error("Process handle missing.".into()));
                    return;
                }
            }
        };
        control.clear();

        match status {
            Ok(status) => {
                let summary = match status.code() {
                    Some(0) => "Completed successfully.".to_string(),
                    Some(code) => format!("Exited with code {code}."),
                    None => "Process was terminated.".to_string(),
                };
                let _ = tx.send(ProcessEvent::Finished(summary));
            }
            Err(error) => {
                let _ = tx.send(ProcessEvent::Error(format!("Could not wait: {error}")));
            }
        }
    });
}

/// Run an in-process job on a background thread; send events on `tx`.
///
/// The job receives the shared cancel flag and a log sender. On success it should
/// return `Ok(())`; on failure `Err(message)`. Cancelled jobs typically return
/// an error containing `"cancelled"`.
pub fn spawn_job<F>(control: ProcessControl, tx: mpsc::Sender<ProcessEvent>, label: String, job: F)
where
    F: FnOnce(CancelFlag, mpsc::Sender<ProcessEvent>) -> Result<(), String> + Send + 'static,
{
    control.begin_job();
    let cancel = control.cancel_flag();
    thread::spawn(move || {
        let _ = tx.send(ProcessEvent::Started(label));
        match job(cancel, tx.clone()) {
            Ok(()) => {
                let _ = tx.send(ProcessEvent::Finished(
                    "Completed successfully.".to_string(),
                ));
            }
            Err(error) => {
                if error.to_ascii_lowercase().contains("cancelled") {
                    let _ = tx.send(ProcessEvent::Finished("Cancelled.".to_string()));
                } else {
                    let _ = tx.send(ProcessEvent::Error(error));
                }
            }
        }
    });
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn format_command(program: &Path, args: &[OsString]) -> String {
    let mut parts = vec![quote(&program.display().to_string())];
    let mut redact_next = false;
    for arg in args {
        let value = arg.to_string_lossy();
        if redact_next {
            parts.push("\"[redacted]\"".into());
            redact_next = false;
            continue;
        }
        redact_next = value == "--cleartext-password" || value == "-x";
        parts.push(quote(&value));
    }
    parts.join(" ")
}

fn quote(value: &str) -> String {
    if value.contains([' ', '\t', '"']) {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_display_quotes_spaces() {
        let text = format_command(
            Path::new("tool path"),
            &[OsString::from("--input"), OsString::from("a b")],
        );
        assert_eq!(text, "\"tool path\" --input \"a b\"");
    }

    #[test]
    fn command_display_redacts_password() {
        let text = format_command(
            Path::new("tool"),
            &[
                OsString::from("--cleartext-password"),
                OsString::from("secret value"),
            ],
        );
        assert_eq!(text, "tool --cleartext-password \"[redacted]\"");
        assert!(!text.contains("secret value"));
    }

    #[test]
    fn cancel_sets_flag_without_child() {
        let control = ProcessControl::default();
        assert!(!control.cancel_flag().load(Ordering::Relaxed));
        control.cancel().unwrap();
        assert!(control.cancel_flag().load(Ordering::Relaxed));
    }
}
