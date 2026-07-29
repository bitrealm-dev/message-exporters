//! Mock-server smoke for auth + JSONL push + journal skip.

use std::fs;
use std::io::Write;
use std::path::Path;

use httpmock::prelude::*;
use message_ir::{
    ConversationDocument, ConversationMeta, ConversationStats, ExportMeta, IrConversationType,
    IrDirection, IrMessage, IrMessageKind, IrParticipant, IrService, SCHEMA_VERSION,
};
use serde_json::json;
use tempfile::tempdir;
use vault_push::{VaultPushConfig, authenticate, run};

fn sample_doc() -> ConversationDocument {
    ConversationDocument {
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
            guid: "guid-1".into(),
            timestamp_unix_ms: 1_400_773_261_000,
            direction: IrDirection::Incoming,
            service: IrService::Sms,
            message_kind: IrMessageKind::Sms,
            sender_handle: Some("+15555550101".into()),
            sender_display_name: Some("Sam".into()),
            subject: None,
            text: "hello vault".into(),
            attachments: vec![],
            imessage: None,
            source: None,
        }],
        packaging_stem_suffix: None,
    }
}

fn write_jsonl(dir: &Path, doc: &ConversationDocument) {
    let stem = doc.filename_stem();
    let path = dir.join(format!("{stem}.jsonl"));
    let mut f = fs::File::create(&path).unwrap();
    let header = json!({
        "schema_version": doc.schema_version,
        "export": doc.export,
        "conversation": doc.conversation,
    });
    writeln!(f, "{}", serde_json::to_string(&header).unwrap()).unwrap();
    for msg in &doc.messages {
        writeln!(f, "{}", serde_json::to_string(msg).unwrap()).unwrap();
    }
}

#[test]
fn authenticate_and_push_text_only_conversation() {
    let server = MockServer::start();
    let _auth = server.mock(|when, then| {
        when.method(GET).path("/v1/auth/check");
        then.status(200).json_body(json!({
            "ok": true,
            "account_id": "acct-1",
            "username": "alice",
            "account_ok": true,
            "sources": ["sms-backup-restore"]
        }));
    });
    let import = server.mock(|when, then| {
        when.method(POST).path("/v1/import");
        then.status(200).json_body(json!({
            "ok": true,
            "source": "sms-backup-restore",
            "account": "acct-1",
            "messages": 1,
            "messages_appended": 1,
            "conversations": 1,
            "attachments": 0,
            "assets_copied": 0,
            "assets_missing": 0,
            "mode": "append"
        }));
    });

    let info = authenticate(&server.base_url(), "mv_test", "alice").unwrap();
    assert_eq!(info.account_id, "acct-1");

    let dir = tempdir().unwrap();
    write_jsonl(dir.path(), &sample_doc());

    let cfg = VaultPushConfig {
        input: dir.path().to_path_buf(),
        base_url: server.base_url(),
        username: "alice".into(),
        key: "mv_test".into(),
        mode: "append".into(),
        continue_on_error: true,
        force: false,
        max_retries: 0,
        batch_size: 50,
        asset_upload_workers: 1,
        report_path: Some(dir.path().join("vault-push-report.json")),
        log_path: Some(dir.path().join("vault-push.log")),
        journal_path: Some(dir.path().join(".vault-import-state.jsonl")),
        cancel: None,
    };
    let report = run(&cfg, None).unwrap();
    assert!(report.ok);
    assert_eq!(report.conversations_ok, 1);
    import.assert();

    // Second run should skip via journal.
    let report2 = run(&cfg, None).unwrap();
    assert!(report2.ok);
    assert_eq!(report2.conversations_skipped, 1);
}
