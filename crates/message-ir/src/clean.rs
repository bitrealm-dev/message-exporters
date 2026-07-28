//! Remove prior IR packaging artifacts under an export directory.

use anyhow::{Context, Result};
use message_mail::clean_previous_mail_output;
use std::fs;
use std::path::Path;

/// Delete previous CSV / JSON / JSONL / meta / `smses.xml` / temps, then mail archives.
///
/// Leaves `attachments/` alone. Safe when `output_dir` does not exist.
pub fn clean_previous_ir_output(output_dir: &Path) -> Result<()> {
    if !output_dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(output_dir)
        .with_context(|| format!("read {}", output_dir.display()))?
    {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !path.is_file() {
            continue;
        }
        let remove = name.ends_with(".csv")
            || name.ends_with(".csv.tmp")
            || name.ends_with(".meta.json")
            || name.ends_with(".meta.json.tmp")
            || name.ends_with(".json")
            || name.ends_with(".json.tmp")
            || name.ends_with(".jsonl")
            || name.ends_with(".jsonl.tmp")
            || name == "smses.xml"
            || name.ends_with(".xml.tmp")
            || name.ends_with(".xml.sbrbody");
        if remove {
            fs::remove_file(&path)
                .with_context(|| format!("remove previous {}", path.display()))?;
        }
    }
    clean_previous_mail_output(output_dir)?;
    Ok(())
}
