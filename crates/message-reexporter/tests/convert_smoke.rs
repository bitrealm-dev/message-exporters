use message_exporters_core::{MediaConfig, ObfuscateConfig, OutputFormat};
use message_ir::{
    ConversationDocument, ConversationMeta, ConversationStats, ExportMeta, ExportTransforms,
    FormatSink, IrConversationType, IrDirection, IrMessage, IrMessageKind, IrParticipant,
    IrService, SCHEMA_VERSION, clean_previous_ir_output, read_conversation_csv,
    read_conversation_json,
};
use message_reexporter::{convert_export, detect_ir_export};
use std::fs;
use std::path::Path;

fn sample_doc() -> ConversationDocument {
    let mut doc = ConversationDocument {
        schema_version: SCHEMA_VERSION,
        export: ExportMeta {
            source: "sms-backup-restore".into(),
            tool: "SMS Backup & Restore".into(),
            tool_version: "10.26.003".into(),
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
            guid: "aabbccddeeff00112233445566778899".into(),
            timestamp_unix_ms: 1_400_773_261_000,
            direction: IrDirection::Incoming,
            service: IrService::Sms,
            message_kind: IrMessageKind::Sms,
            sender_handle: Some("+15555550101".into()),
            sender_display_name: Some("Sam".into()),
            subject: None,
            text: "hello reexport".into(),
            attachments: vec![],
            imessage: None,
            source: None,
        }],
        packaging_stem_suffix: None,
    };
    doc.finalize_stats();
    doc
}

fn write_fixture(dir: &Path, format: OutputFormat) {
    fs::create_dir_all(dir).unwrap();
    clean_previous_ir_output(dir).unwrap();
    let mut sink = FormatSink::open(dir, format, ExportTransforms::none()).unwrap();
    sink.write_document(&sample_doc()).unwrap();
    sink.finish().unwrap();
}

#[test]
fn detect_json_and_convert_to_csv() {
    let src = tempfile::tempdir().unwrap();
    write_fixture(src.path(), OutputFormat::Json);
    assert_eq!(
        detect_ir_export(src.path()).unwrap().format,
        OutputFormat::Json
    );

    let dest = tempfile::tempdir().unwrap();
    let report = convert_export(
        src.path(),
        dest.path(),
        OutputFormat::Csv,
        &MediaConfig::default(),
        &ObfuscateConfig::default(),
    )
    .unwrap();
    assert_eq!(report.conversations, 1);
    assert_eq!(report.detected_format, "json");

    let csv = fs::read_dir(dest.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("csv"))
        .expect("csv");
    let back = read_conversation_csv(&csv).unwrap();
    assert_eq!(back.messages[0].text, "hello reexport");
}

#[test]
fn convert_csv_to_json() {
    let src = tempfile::tempdir().unwrap();
    write_fixture(src.path(), OutputFormat::Csv);
    let dest = tempfile::tempdir().unwrap();
    convert_export(
        src.path(),
        dest.path(),
        OutputFormat::Json,
        &MediaConfig::default(),
        &ObfuscateConfig::default(),
    )
    .unwrap();
    let json = fs::read_dir(dest.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("json")
                && !p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".meta.json"))
        })
        .expect("json");
    let back = read_conversation_json(&json).unwrap();
    assert_eq!(back.messages[0].text, "hello reexport");
}

#[test]
fn convert_json_to_xml() {
    let src = tempfile::tempdir().unwrap();
    write_fixture(src.path(), OutputFormat::Json);
    let dest = tempfile::tempdir().unwrap();
    convert_export(
        src.path(),
        dest.path(),
        OutputFormat::Xml,
        &MediaConfig::default(),
        &ObfuscateConfig::default(),
    )
    .unwrap();
    assert!(dest.path().join("smses.xml").is_file());
}

#[test]
fn mixed_formats_error() {
    let src = tempfile::tempdir().unwrap();
    write_fixture(src.path(), OutputFormat::Json);
    // Add a second format class.
    let mut sink =
        FormatSink::open(src.path(), OutputFormat::Csv, ExportTransforms::none()).unwrap();
    sink.write_document(&sample_doc()).unwrap();
    sink.finish().unwrap();
    let err = detect_ir_export(src.path()).unwrap_err().to_string();
    assert!(err.contains("mixed"), "{err}");
}

#[test]
fn same_path_errors() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path(), OutputFormat::Json);
    let err = convert_export(
        dir.path(),
        dir.path(),
        OutputFormat::Csv,
        &MediaConfig::default(),
        &ObfuscateConfig::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("different"), "{err}");
}

#[test]
fn meta_json_does_not_count_as_json_export() {
    let src = tempfile::tempdir().unwrap();
    write_fixture(src.path(), OutputFormat::Csv);
    assert_eq!(
        detect_ir_export(src.path()).unwrap().format,
        OutputFormat::Csv
    );
}
