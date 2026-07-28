//! Attachment media + obfuscate transforms applied before IR projection.

use crate::{ConversationDocument, IrAttachment, IrDirection, IrParticipant};
use anyhow::Result;
use message_exporters_core::{MediaConfig, ObfuscateConfig};
use message_media::{CompressOptions, MediaMode, MediaReport};
use message_obfuscate::{
    classify_attachment, materialize_placeholders, placeholder_rel_path, resolve_obfuscator,
    Obfuscator,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Options passed into [`crate::FormatSink`] for media and obfuscation.
#[derive(Debug, Clone)]
pub struct ExportTransforms {
    pub media: MediaMode,
    pub compress: CompressOptions,
    pub obfuscate: bool,
    pub obfuscate_seed: Option<String>,
}

impl Default for ExportTransforms {
    fn default() -> Self {
        Self {
            media: MediaMode::Clone,
            compress: CompressOptions::default(),
            obfuscate: false,
            obfuscate_seed: None,
        }
    }
}

impl ExportTransforms {
    pub fn from_configs(media: &MediaConfig, obfuscate: &ObfuscateConfig) -> Self {
        Self {
            media: media.mode,
            compress: media.compress.clone(),
            obfuscate: obfuscate.enabled || obfuscate.seed.is_some(),
            obfuscate_seed: obfuscate.seed.clone(),
        }
    }

    pub fn none() -> Self {
        Self::default()
    }

    pub fn needs_media_tools(&self) -> bool {
        self.media.needs_tools()
    }

    pub fn copies_attachments(&self) -> bool {
        self.media.copies_attachments()
    }
}

pub(crate) fn apply_media_remap(doc: &mut ConversationDocument, remap: &HashMap<String, String>) {
    if remap.is_empty() {
        return;
    }
    for msg in &mut doc.messages {
        for att in &mut msg.attachments {
            if let Some(path) = att.path.as_mut() {
                if let Some(new_rel) = remap.get(path.as_str()) {
                    *path = new_rel.clone();
                    if let Some(mime) = mime_for_rel(new_rel) {
                        att.mime_type = Some(mime);
                    }
                }
            }
        }
    }
}

pub(crate) fn reload_attachment_bytes(doc: &mut ConversationDocument, output_dir: &Path) {
    for msg in &mut doc.messages {
        for att in &mut msg.attachments {
            if let Some(rel) = att.path.as_deref() {
                let path = output_dir.join(rel);
                if path.is_file() {
                    att.bytes = fs::read(&path).ok();
                }
            }
        }
    }
}

pub(crate) fn clear_attachments_when_disabled(doc: &mut ConversationDocument, mode: MediaMode) {
    if !matches!(mode, MediaMode::Disabled) {
        return;
    }
    for msg in &mut doc.messages {
        for att in &mut msg.attachments {
            att.path = None;
            att.bytes = None;
            att.digest_sha256 = None;
        }
    }
}

pub(crate) fn obfuscate_document(doc: &mut ConversationDocument, anon: &mut Obfuscator) {
    doc.conversation.chat_identifier = anon.obfuscate_handle(&doc.conversation.chat_identifier);
    if let Some(title) = doc.conversation.group_title.as_mut() {
        *title = anon.obfuscate_mixed_field(title);
    }
    for p in &mut doc.conversation.participants {
        obfuscate_participant(p, anon);
    }
    if let Some(h) = doc.export.owner_handle.as_mut() {
        *h = anon.obfuscate_handle(h);
    }
    if let Some(n) = doc.export.owner_display_name.as_mut() {
        *n = anon.obfuscate_display_name(n);
    }
    for msg in &mut doc.messages {
        if let Some(h) = msg.sender_handle.as_mut() {
            *h = anon.obfuscate_handle(h);
        }
        if let Some(n) = msg.sender_display_name.as_mut() {
            if msg.direction == IrDirection::Outgoing && n == "Me" {
                // Keep the conventional outgoing label.
            } else {
                *n = anon.obfuscate_display_name(n);
            }
        }
        if let Some(s) = msg.subject.as_mut() {
            *s = anon.obfuscate_text(s);
        }
        msg.text = anon.obfuscate_text(&msg.text);
        if let Some(im) = msg.imessage.as_mut() {
            if let Some(a) = im.announcement.as_mut() {
                *a = anon.obfuscate_text(a);
            }
        }
        for att in &mut msg.attachments {
            obfuscate_attachment(att);
        }
    }
}

fn obfuscate_participant(p: &mut IrParticipant, anon: &mut Obfuscator) {
    p.handle = anon.obfuscate_handle(&p.handle);
    if let Some(n) = p.display_name.as_mut() {
        *n = anon.obfuscate_display_name(n);
    }
}

fn obfuscate_attachment(att: &mut IrAttachment) {
    let class = classify_attachment(att.mime_type.as_deref(), att.path.as_deref());
    let rel = placeholder_rel_path(class);
    att.path = Some(rel.to_string());
    let ext = Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    att.original_name = Some(format!("attachment.{ext}"));
    if att
        .transcription
        .as_deref()
        .is_some_and(|s| !s.is_empty())
    {
        att.transcription = Some("[redacted]".into());
    }
    att.digest_sha256 = None;
    att.bytes = None;
}

fn mime_for_rel(rel: &str) -> Option<String> {
    let ext = Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Some(
        match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "mp4" | "m4v" => "video/mp4",
            "mov" => "video/quicktime",
            "mp3" => "audio/mpeg",
            "m4a" => "audio/mp4",
            _ => return None,
        }
        .into(),
    )
}

pub(crate) struct TransformOutcome {
    pub media: MediaReport,
    pub obfuscated_docs: usize,
}

pub(crate) fn apply_transforms(
    docs: &mut [ConversationDocument],
    output_dir: &Path,
    transforms: &ExportTransforms,
    load_bytes: bool,
) -> Result<TransformOutcome> {
    for doc in docs.iter_mut() {
        clear_attachments_when_disabled(doc, transforms.media);
    }

    let (media, remap) =
        message_media::process_attachments_dir(output_dir, transforms.media, &transforms.compress)?;
    if !remap.is_empty() {
        for doc in docs.iter_mut() {
            apply_media_remap(doc, &remap);
        }
    }

    let mut obfuscated_docs = 0usize;
    if transforms.obfuscate {
        materialize_placeholders(output_dir)?;
        let mut anon = resolve_obfuscator(transforms.obfuscate_seed.as_deref())?;
        for doc in docs.iter_mut() {
            obfuscate_document(doc, &mut anon);
            obfuscated_docs += 1;
        }
    }

    if load_bytes {
        for doc in docs.iter_mut() {
            reload_attachment_bytes(doc, output_dir);
        }
    }

    Ok(TransformOutcome {
        media,
        obfuscated_docs,
    })
}
