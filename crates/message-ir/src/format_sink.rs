//! Unified per-export writer for IR packaging formats.

use crate::write_sbr::SbrBackupSession;
use crate::{write_format, ConversationDocument};
use anyhow::Result;
use message_exporters_core::OutputFormat;
use std::path::{Path, PathBuf};

/// Writes conversations in the requested [`OutputFormat`].
///
/// For CSV/EML/MBOX/JSON/JSONL each [`write_document`](Self::write_document)
/// call projects one conversation. For XML, documents are appended into a
/// single `smses.xml` and finalized in [`finish`](Self::finish).
pub struct FormatSink {
    output_dir: PathBuf,
    format: OutputFormat,
    sbr: Option<SbrBackupSession>,
}

impl FormatSink {
    pub fn open(output_dir: &Path, format: OutputFormat) -> Result<Self> {
        let sbr = if format.is_sbr_xml() {
            Some(SbrBackupSession::create(output_dir)?)
        } else {
            None
        };
        Ok(Self {
            output_dir: output_dir.to_path_buf(),
            format,
            sbr,
        })
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    pub fn write_document(&mut self, doc: &ConversationDocument) -> Result<()> {
        if let Some(session) = self.sbr.as_mut() {
            session.append_document(doc)
        } else {
            write_format(&self.output_dir, self.format, doc)?;
            Ok(())
        }
    }

    /// Finalize XML backup (no-op for other formats).
    pub fn finish(self) -> Result<Option<PathBuf>> {
        match self.sbr {
            Some(session) => Ok(Some(session.finish()?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ConversationMeta, ConversationStats, ExportMeta, IrConversationType, IrDirection,
        IrMessage, IrMessageKind, IrParticipant, IrService, SCHEMA_VERSION,
    };
    use std::fs;

    fn tiny_doc(text: &str) -> ConversationDocument {
        let mut doc = ConversationDocument {
            schema_version: SCHEMA_VERSION,
            export: ExportMeta {
                source: "test".into(),
                tool: "test".into(),
                tool_version: "0".into(),
                owner_handle: Some("+15555550100".into()),
                owner_display_name: Some("Me".into()),
            },
            conversation: ConversationMeta {
                chat_identifier: "+15555550101".into(),
                conversation_type: IrConversationType::Individual,
                group_title: None,
                participants: vec![IrParticipant {
                    handle: "+15555550101".into(),
                    display_name: Some("Sam".into()),
                }],
                stats: ConversationStats::default(),
            },
            messages: vec![IrMessage {
                guid: "guid-1".into(),
                timestamp_unix_ms: 1_400_773_261_000,
                direction: IrDirection::Incoming,
                service: IrService::Sms,
                message_kind: IrMessageKind::Sms,
                sender_handle: Some("+15555550101".into()),
                sender_display_name: Some("Sam".into()),
                subject: None,
                text: text.into(),
                attachments: vec![],
                imessage: None,
                source: None,
            }],
            packaging_stem_suffix: None,
        };
        doc.finalize_stats();
        doc
    }

    #[test]
    fn format_sink_csv_writes_per_conversation() {
        let tmp = tempfile::tempdir().unwrap();
        let mut sink = FormatSink::open(tmp.path(), OutputFormat::Csv).unwrap();
        sink.write_document(&tiny_doc("hello")).unwrap();
        sink.finish().unwrap();
        assert!(tmp.path().join("+15555550101.csv").is_file());
    }

    #[test]
    fn format_sink_xml_merges_documents() {
        let tmp = tempfile::tempdir().unwrap();
        let mut sink = FormatSink::open(tmp.path(), OutputFormat::Xml).unwrap();
        sink.write_document(&tiny_doc("one")).unwrap();
        let mut doc2 = tiny_doc("two");
        doc2.messages[0].guid = "guid-2".into();
        doc2.messages[0].timestamp_unix_ms = 1_400_773_262_000;
        sink.write_document(&doc2).unwrap();
        let path = sink.finish().unwrap().expect("smses.xml");
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains(r#"count="2""#));
        assert!(text.contains("one"));
        assert!(text.contains("two"));
    }
}
