# Canonical message IR (JSON / JSONL)

**Intermediate representation** after source parse and before CSV / EML / MBOX / JSON / JSONL packaging.

Typed model: [`crates/message-ir/`](../crates/message-ir/). On-disk forms:

- **JSON** — one pretty-printed `<conversation-stem>.json` per chat (`ConversationDocument`)
- **JSONL** — one `<conversation-stem>.jsonl` per chat: header line, then one `IrMessage` per line

Stem rules match CSV filenames (including optional `filename_suffix`, e.g. `__whatsapp`).

## Status

- **IR-backed** (`ConversationDocument` → `message_ir::write_format`, `--format csv|eml|mbox|json|jsonl`): all exporters, including iMessage (`imessage-ir-exporter`).
- **Schema version 2** (breaking vs v1): nested objects/arrays instead of stringified `*_json` bags; no `source.date_ms`.

## Document shape (`schema_version: 2`)

```json
{
  "schema_version": 2,
  "export": {
    "source": "sms-backup-restore",
    "tool": "SMS Backup & Restore",
    "tool_version": "10.26.003",
    "owner_handle": "+15555550100",
    "owner_display_name": null
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
      "service": "sms",
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
        "contact_name": "Sam",
        "android_type": "1",
        "fields": { "address": "+15555550101" }
      }
    }
  ]
}
```

| Tier | Contents |
|------|----------|
| Core | conversation + message fields shared with CSV / mail |
| `imessage` | optional object: nested `parts` / `edits` / `tapbacks` / `app`, plus scalars (`is_reply`, `read_receipt_rfc3339`, …) |
| `source` | vendor leftovers: `contact_name`, `android_type`, `fields` (object) |

### Identity

- Outgoing rows leave `sender_handle` / `sender_display_name` blank by design.
- Owner identity lives on `export.owner_handle` and optional `export.owner_display_name` (iMessage caller-id / `"Me"`).

### Attachments

Attachment **bytes** are never stored in JSON/JSONL (`#[serde(skip)]`). Projectors read `attachments/` by `path`, or use in-memory bytes when the exporter kept them for EML. JSON alone is not enough to rebuild mail with media.

### Vocabulary (normalized at emit)

- `conversation_type`: `individual` \| `group`
- `service`: lowercase (`sms`, `imessage`, `whatsapp`, …)
- `message_kind`: exporter-specific lowercase/snake strings (`sms`, `mms`, `tapback`, …)

### `filename_suffix`

When set (e.g. `__whatsapp`), it is serialized on `conversation` and incorporated into the on-disk stem so the JSON body matches the filename.

## JSONL layout

```text
{"schema_version":2,"export":{…},"conversation":{…}}
{"guid":"…","timestamp_unix_ms":…, …}
{"guid":"…","timestamp_unix_ms":…, …}
…
```

Line 1 is the header (no `messages` array). Each following line is one `IrMessage` (same nested v2 shape).

## Projectors

| Format | Writer |
|--------|--------|
| JSON | pretty-printed `ConversationDocument` |
| JSONL | header + one message per line |
| CSV | shared SBR-compatible columns (+ Apple columns when `export.source == "imessage"`); nested bags stringified into `xml_fields_json` / `parts_json` / … cells |
| EML / MBOX | IR → `MailMessage` → [`message-mail`](../crates/message-mail/) |

## Related

- [MAIL_ARCHIVE.md](MAIL_ARCHIVE.md) — EML/MBOX packaging
- [csv-output.md](src/csv-output.md) — CSV conventions
- [EXPORTER_MATRIX.md](EXPORTER_MATRIX.md)
