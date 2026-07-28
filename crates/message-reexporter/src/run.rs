//! Library entrypoint used by CLI and GUI.

use crate::convert::convert_export;
use anyhow::{bail, Result};
use message_exporters_core::ExporterConfig;

#[derive(Debug, Default)]
pub struct RunResult {
    pub messages: Vec<String>,
}

/// Run re-export from [`ExporterConfig`] (`inputs[0]` → `output`, `output_format`).
pub fn run(config: &ExporterConfig) -> Result<RunResult> {
    let input = config
        .require_input()
        .map_err(anyhow::Error::msg)?;
    if config.output.as_os_str().is_empty() {
        bail!("output directory is required");
    }
    let report = convert_export(
        input,
        &config.output,
        config.output_format,
        &config.media,
        &config.obfuscate,
    )?;
    Ok(RunResult {
        messages: report.log_lines(),
    })
}
