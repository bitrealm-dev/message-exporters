use iced::futures::{SinkExt, Stream};
use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum ProcessEvent {
    Started(String),
    Log(String),
    Finished(String),
    Error(String),
}

#[derive(Debug, Clone, Default)]
pub struct ProcessControl {
    child: Arc<Mutex<Option<Child>>>,
}

impl ProcessControl {
    pub fn cancel(&self) -> Result<(), String> {
        let mut guard = self.child.lock().map_err(|_| "process lock poisoned")?;
        let Some(child) = guard.as_mut() else {
            return Err("No exporter is running.".into());
        };
        child
            .kill()
            .map_err(|error| format!("Could not cancel exporter: {error}"))
    }

    fn clear(&self) {
        if let Ok(mut child) = self.child.lock() {
            *child = None;
        }
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
        "Could not find {executable}. Build the exporter in the same profile as the GUI, \
         add it to PATH, or set MESSAGE_EXPORTERS_BIN. Tried: {}",
        tried
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

pub fn run(
    program: PathBuf,
    args: Vec<OsString>,
    control: ProcessControl,
) -> impl Stream<Item = ProcessEvent> {
    iced::stream::channel(100, async move |mut output| {
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
                let _ = output
                    .send(ProcessEvent::Error(format!(
                        "Could not start {}: {error}",
                        program.display()
                    )))
                    .await;
                return;
            }
        };
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        if let Ok(mut slot) = control.child.lock() {
            *slot = Some(child);
        } else {
            let _ = output
                .send(ProcessEvent::Error("Process lock poisoned.".into()))
                .await;
            return;
        }

        let _ = output.send(ProcessEvent::Started(command_display)).await;
        let (tx, rx) = mpsc::channel::<String>();
        if let Some(stdout) = stdout {
            spawn_reader(stdout, "[stdout]", tx.clone());
        }
        if let Some(stderr) = stderr {
            spawn_reader(stderr, "[stderr]", tx.clone());
        }
        drop(tx);

        loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => {
                    if output.send(ProcessEvent::Log(line)).await.is_err() {
                        let _ = control.cancel();
                        control.clear();
                        return;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {}
            }

            let status = match process_status(&control) {
                Ok(status) => status,
                Err(error) => {
                    let _ = output.send(ProcessEvent::Error(error)).await;
                    return;
                }
            };

            if let Some(status) = status {
                let remaining = rx.try_iter().collect::<Vec<_>>();
                for line in remaining {
                    let _ = output.send(ProcessEvent::Log(line)).await;
                }
                control.clear();
                let summary = match status.code() {
                    Some(0) => "Exporter completed successfully.".to_string(),
                    Some(code) => format!("Exporter exited with code {code}."),
                    None => "Exporter was terminated.".to_string(),
                };
                let _ = output.send(ProcessEvent::Finished(summary)).await;
                return;
            }
        }
    })
}

fn process_status(control: &ProcessControl) -> Result<Option<std::process::ExitStatus>, String> {
    let mut guard = control
        .child
        .lock()
        .map_err(|_| "Process lock poisoned.".to_string())?;
    let Some(child) = guard.as_mut() else {
        return Ok(None);
    };
    child
        .try_wait()
        .map_err(|error| format!("Could not query exporter status: {error}"))
}

fn spawn_reader<R>(reader: R, prefix: &'static str, sender: mpsc::Sender<String>)
where
    R: std::io::Read + Send + 'static,
{
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            match line {
                Ok(line) => {
                    let _ = sender.send(format!("{prefix} {line}"));
                }
                Err(error) => {
                    let _ = sender.send(format!("{prefix} read error: {error}"));
                    break;
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
}
