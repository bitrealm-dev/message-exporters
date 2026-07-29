//! Load detected IR export and write another packaging format.

use crate::detect::{DetectedExport, detect_ir_export, list_artifacts};
use anyhow::{Context, Result, bail};
use message_exporters_core::{MediaConfig, ObfuscateConfig, OutputFormat};
use message_ir::{
    ConversationDocument, ExportTransforms, FormatSink, FormatSinkResult, clean_previous_ir_output,
    read_conversation_csv, read_conversation_eml_dir, read_conversation_json,
    read_conversation_jsonl, read_conversation_mbox,
};
use sms_backup_restore_exporter::load_documents_from_xml;
use std::fs;
use std::path::Path;

#[derive(Debug, Default)]
pub struct ReexportReport {
    pub detected_format: String,
    pub conversations: usize,
    pub sink: FormatSinkResult,
}

impl ReexportReport {
    pub fn log_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("Detected input format: {}", self.detected_format),
            format!("Conversations: {}", self.conversations),
        ];
        lines.extend(self.sink.log_lines());
        lines
    }
}

/// Convert an existing Message Exporters output directory to `output_format`.
pub fn convert_export(
    input_dir: &Path,
    output_dir: &Path,
    output_format: OutputFormat,
    media: &MediaConfig,
    obfuscate: &ObfuscateConfig,
) -> Result<ReexportReport> {
    let input_canon = fs::canonicalize(input_dir)
        .with_context(|| format!("canonicalize input {}", input_dir.display()))?;
    if output_dir.exists() {
        let out_canon = fs::canonicalize(output_dir)
            .with_context(|| format!("canonicalize output {}", output_dir.display()))?;
        if input_canon == out_canon {
            bail!("input and output directories must be different");
        }
    }

    let detected = detect_ir_export(input_dir)?;
    let transforms = ExportTransforms::from_configs(media, obfuscate);
    let copy_attachments = transforms.copies_attachments();

    fs::create_dir_all(output_dir).with_context(|| format!("create {}", output_dir.display()))?;
    clean_previous_ir_output(output_dir)?;

    if copy_attachments {
        copy_attachments_dir(input_dir, output_dir)?;
    }

    let docs = load_documents(input_dir, &detected, output_dir, copy_attachments)?;
    if docs.is_empty() {
        bail!("no conversations loaded from {}", input_dir.display());
    }

    let mut sink = FormatSink::open(output_dir, output_format, transforms)?;
    for doc in &docs {
        sink.write_document(doc)?;
    }
    let sink_result = sink.finish()?;

    Ok(ReexportReport {
        detected_format: detected.format.as_str().to_string(),
        conversations: docs.len(),
        sink: sink_result,
    })
}

fn load_documents(
    input_dir: &Path,
    detected: &DetectedExport,
    output_dir: &Path,
    copy_attachments: bool,
) -> Result<Vec<ConversationDocument>> {
    match detected.format {
        OutputFormat::Xml => {
            let attachments_dir = output_dir.join("attachments");
            let (docs, report) =
                load_documents_from_xml(input_dir, &[], Some(&attachments_dir), copy_attachments)?;
            if !report.errors.is_empty() {
                for err in report.errors.iter().take(5) {
                    eprintln!("xml warning: {err}");
                }
            }
            Ok(docs)
        }
        OutputFormat::Json => {
            let mut docs = Vec::new();
            for path in list_artifacts(input_dir, OutputFormat::Json)? {
                docs.push(read_conversation_json(&path)?);
            }
            Ok(docs)
        }
        OutputFormat::Jsonl => {
            let mut docs = Vec::new();
            for path in list_artifacts(input_dir, OutputFormat::Jsonl)? {
                docs.push(read_conversation_jsonl(&path)?);
            }
            Ok(docs)
        }
        OutputFormat::Csv => {
            let mut docs = Vec::new();
            for path in list_artifacts(input_dir, OutputFormat::Csv)? {
                docs.push(read_conversation_csv(&path)?);
            }
            Ok(docs)
        }
        OutputFormat::Mbox => {
            let mut docs = Vec::new();
            for path in list_artifacts(input_dir, OutputFormat::Mbox)? {
                docs.push(read_conversation_mbox(&path)?);
            }
            Ok(docs)
        }
        OutputFormat::Eml => {
            let mut docs = Vec::new();
            for path in list_artifacts(input_dir, OutputFormat::Eml)? {
                docs.push(read_conversation_eml_dir(&path)?);
            }
            Ok(docs)
        }
    }
}

fn copy_attachments_dir(input_dir: &Path, output_dir: &Path) -> Result<()> {
    let src = input_dir.join("attachments");
    if !src.is_dir() {
        return Ok(());
    }
    let dest = output_dir.join("attachments");
    fs::create_dir_all(&dest)?;
    copy_dir_recursive(&src, &dest)?;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    for entry in fs::read_dir(src).with_context(|| format!("read {}", src.display()))? {
        let entry = entry?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            fs::create_dir_all(&to)?;
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)
                .with_context(|| format!("copy {} → {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}
