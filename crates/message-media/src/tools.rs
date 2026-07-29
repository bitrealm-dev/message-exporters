use std::process::{Command, Stdio};

use anyhow::{Result, bail};

pub fn ffmpeg_available() -> bool {
    command_ok("ffmpeg", &["-version"]) && command_ok("ffprobe", &["-version"])
}

pub(crate) fn require_ffmpeg() -> Result<()> {
    if ffmpeg_available() {
        Ok(())
    } else {
        bail!(
            "ffmpeg and ffprobe are required for --media-mode convert/compress. \
             Install ffmpeg and ensure both are on PATH."
        )
    }
}

fn command_ok(bin: &str, args: &[&str]) -> bool {
    Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub(crate) fn run_ffmpeg(args: &[String]) -> Result<()> {
    let status = Command::new("ffmpeg")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        bail!("ffmpeg failed ({status})")
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct Probe {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub bitrate: u64,
}

pub(crate) fn probe_video(path: &std::path::Path) -> Result<Probe> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,width,height,bit_rate",
            "-of",
            "csv=p=0",
            path.to_str().unwrap_or(""),
        ])
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        bail!("ffprobe failed for {}", path.display());
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = line.trim().split(',').collect();
    let codec = parts.first().copied().unwrap_or("").to_ascii_lowercase();
    let width = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let height = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    let bitrate = parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    Ok(Probe {
        codec,
        width,
        height,
        bitrate,
    })
}
