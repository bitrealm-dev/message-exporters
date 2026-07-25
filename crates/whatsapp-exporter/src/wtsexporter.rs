//! Locate and run the external `wtsexporter` CLI.

use anyhow::{bail, Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PINNED_HINT: &str = "whatsapp-chat-exporter>=0.13";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Android,
    Ios,
}

impl Platform {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "android" | "a" => Some(Self::Android),
            "ios" | "iphone" | "ipad" | "i" => Some(Self::Ios),
            _ => None,
        }
    }

    pub fn as_flag(self) -> &'static str {
        match self {
            Self::Android => "-a",
            Self::Ios => "-i",
        }
    }
}

#[derive(Debug, Clone)]
pub struct WtsexporterArgs {
    pub platform: Platform,
    /// Working directory for wtsexporter defaults (`msgstore.db`, etc.).
    pub input: PathBuf,
    /// Key file path or crypt15 hex string (`-k`).
    pub key: Option<String>,
    pub backup: Option<PathBuf>,
    pub wa: Option<PathBuf>,
    pub media: Option<PathBuf>,
    pub db: Option<PathBuf>,
    pub business: bool,
    pub move_media: bool,
}

/// Resolve `wtsexporter`: `WTSEXPORTER` → sibling of this exe → `MESSAGE_EXPORTERS_BIN` → `PATH`.
pub fn resolve_wtsexporter() -> Result<PathBuf> {
    if let Some(explicit) = env::var_os("WTSEXPORTER") {
        let path = PathBuf::from(explicit);
        if path.is_file() {
            return Ok(path);
        }
        bail!(
            "WTSEXPORTER is set but not a file: {}. Install with \
             pip install '{PINNED_HINT}' or place the release binary beside this tool.",
            path.display()
        );
    }

    let executable = if cfg!(windows) {
        "wtsexporter.exe"
    } else {
        "wtsexporter"
    };
    let mut tried = Vec::new();

    if let Ok(current) = env::current_exe()
        && let Some(dir) = current.parent()
    {
        let sibling = dir.join(executable);
        tried.push(sibling.clone());
        if sibling.is_file() {
            return Ok(sibling);
        }
    }

    if let Some(extra) = env::var_os("MESSAGE_EXPORTERS_BIN") {
        let candidate = PathBuf::from(extra).join(executable);
        tried.push(candidate.clone());
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            let candidate = directory.join(executable);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    bail!(
        "Could not find {executable}. Install with: pip install '{PINNED_HINT}' \
         (or pip install 'whatsapp-chat-exporter[android_backup,crypt15]'), \
         put the KnugiHK release binary next to this tool / in MESSAGE_EXPORTERS_BIN, \
         or set WTSEXPORTER. Tried: {}",
        tried
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// Run wtsexporter; write JSON to `json_out`. Returns stderr+stdout for logging.
pub fn run_wtsexporter(bin: &Path, args: &WtsexporterArgs, json_out: &Path) -> Result<String> {
    let (cwd, db_arg) = resolve_cwd_and_db(&args.input, args.db.as_deref())?;
    let out_dir = json_out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("create {}", out_dir.display()))?;

    let mut cmd = Command::new(bin);
    cmd.current_dir(&cwd)
        .arg(args.platform.as_flag())
        .arg("--no-html")
        .arg("--no-banner")
        .arg("-o")
        .arg(out_dir)
        .arg("-j")
        .arg(json_out)
        // wtsexporter uses tqdm; without this, progress bars spam piped capture
        // (GUI only shows the dump after the process exits).
        .env("TQDM_DISABLE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(db) = db_arg {
        cmd.arg("-d").arg(db);
    }
    if let Some(key) = args.key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        cmd.arg("-k").arg(key);
    }
    push_opt(&mut cmd, "-b", args.backup.as_deref());
    push_opt(&mut cmd, "-w", args.wa.as_deref());
    push_opt(&mut cmd, "-m", args.media.as_deref());
    if args.business {
        cmd.arg("--business");
    }
    if args.move_media {
        cmd.arg("-c");
    }

    let output = cmd.output().map_err(|err| {
        let hint = if err.kind() == std::io::ErrorKind::NotFound {
            " (often a broken pipx/venv shim: the script exists but its Python interpreter does not — try `pipx reinstall whatsapp-chat-exporter` or set WTSEXPORTER to a working binary)"
        } else {
            ""
        };
        anyhow::anyhow!("spawn {}: {err}{hint}", bin.display())
    })?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        bail!(
            "wtsexporter failed ({}){}\n{}",
            output.status,
            if combined.trim().is_empty() {
                ""
            } else {
                ":"
            },
            combined.trim()
        );
    }
    if !json_out.is_file() {
        bail!(
            "wtsexporter finished but JSON missing at {}. Output:\n{}",
            json_out.display(),
            combined.trim()
        );
    }
    Ok(combined)
}

fn push_opt(cmd: &mut Command, flag: &str, path: Option<&Path>) {
    if let Some(p) = path {
        cmd.arg(flag).arg(p);
    }
}

fn resolve_cwd_and_db(
    input: &Path,
    db: Option<&Path>,
) -> Result<(PathBuf, Option<PathBuf>)> {
    if let Some(db) = db {
        let cwd = if input.is_dir() {
            input.to_path_buf()
        } else {
            input
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        };
        return Ok((cwd, Some(db.to_path_buf())));
    }
    if input.is_file() {
        let cwd = input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        return Ok((cwd, Some(input.to_path_buf())));
    }
    if input.is_dir() {
        return Ok((input.to_path_buf(), None));
    }
    bail!("input path does not exist: {}", input.display());
}
