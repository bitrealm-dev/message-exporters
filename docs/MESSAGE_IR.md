# Canonical message IR (JSON)

**Intermediate representation** after source parse and before CSV / EML / MBOX / JSON packaging.

Typed model: [`crates/message-ir/`](../crates/message-ir/). On-disk form: one `<conversation-stem>.json` per chat (same stem rules as CSV).

## Status

- **SMS Backup & Restore** builds `ConversationDocument` then projects with `message_ir::write_format` (`--format csv|eml|mbox|json`).
- Other exporters still write formats directly; JSON bails with a clear error until migrated.
- iPhone CSV still uses `imessage-exporter`; EML/MBOX use `imessage-mail-exporter` (IR for Apple is a follow-up).

## Document shape (`schema_version: 1`)

```json
{
  "schema_version": 1,
  "export": {
    "source": "sms-backup-restore",
    "tool": "SMS Backup & Restore",
    "tool_version": "10.26.003",
    "owner_handle": "+15555550100"
  },
  "conversation": {
    "chat_identifier": "+15555550101",
    "conversation_type": "individual",
    "group_title": null,
    "participants": [{ "handle": "+15555550101", "display_name": "Sam" }],
    "filename_suffix": null
  },
  "messages": [
    {
      "guid": "…",
      "timestamp_unix_ms": 1400773261000,
      "direction": "incoming",
      "service": "SMS",
      "message_kind": "sms",
      "sender_handle": "+15555550101",
      "sender_display_name": "Sam",
      "subject": null,
      "text": "Hello",
      "attachments": [
        {
          "path": "attachments/…",
          "original_name": "photo.jpg",
          "mime_type": "image/jpeg",
          "digest_sha256": "…",
          "is_sticker": false
        }
      ],
      "source": {
        "date_ms": "1400773261000",
        "contact_name": "Sam",
        "android_type": "1",
        "xml_fields_json": "{…}"
      }
    }
  ]
}
```

| Tier | Contents |
|------|----------|
| Core | conversation + message fields shared with CSV / mail |
| `imessage` | optional object for Apple extensions (parts, tapbacks, …) |
| `source` | vendor leftovers (Android types, XML bags, …) |

Attachment **bytes** are not stored in JSON; projectors read `attachments/` by `path` (or use in-memory bytes when the exporter kept them for EML).

## Projectors

| Format | Writer |
|--------|--------|
| JSON | pretty-printed `ConversationDocument` |
| CSV | shared SBR-compatible columns from IR (+ `source` fields) |
| EML / MBOX | IR → `MailMessage` → [`message-mail`](../crates/message-mail/) |

## Related

- [MAIL_ARCHIVE.md](MAIL_ARCHIVE.md) — EML/MBOX packaging
- [csv-output.md](src/csv-output.md) — CSV conventions
- [EXPORTER_MATRIX.md](EXPORTER_MATRIX.md)
