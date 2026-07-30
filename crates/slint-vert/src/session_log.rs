//! Timestamped session log file under the system temp directory
//! (mirrors `message-exporters-gui`).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use chrono::Local;

pub struct SessionLog {
    pub name: String,
    pub path: PathBuf,
}

impl SessionLog {
    pub fn new() -> Self {
        let name = Local::now()
            .format("slint-vert-%Y-%m-%d_%H%M%S.log")
            .to_string();
        let path = std::env::temp_dir().join(&name);
        let _ = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path);
        Self { name, path }
    }

    pub fn truncate(&self) {
        let _ = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path);
    }

    pub fn append(&self, line: &str) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "{line}");
        }
    }
}
