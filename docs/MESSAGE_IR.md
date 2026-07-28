# Canonical message IR (JSON / JSONL)

**Intermediate representation** after source parse and before CSV / EML / MBOX / JSON / JSONL packaging.

Typed model: [`crates/message-ir/`](../crates/message-ir/). On-disk forms:

- **JSON** — one pretty-printed `<conversation-stem>.json` per chat (`ConversationDocument`)
- **JSONL** — one `<conversation-stem>.jsonl` per chat: header line, then one `IrMessage` per line

Stem rules match CSV filenames. Packaging-only suffixes (e.g. `__whatsapp`) affect the on-disk stem but are **not** serialized in the JSON body.

## Status

- **IR-backed** (`ConversationDocument` → `message_ir::write_format`, `--format csv|eml|mbox|json|jsonl`): all exporters, including iMessage (`imessage-ir-exporter`).
- **Schema version 3 only** (breaking). Typed enums/bags, filled outgoing identity, conversation stats, stable null/`[]` keys. Older IR is not read — regenerate exports after schema changes.

## Document shape (`schema_version: 3`)

```json
{
  "schema_version": 3,
  "export": {
    "source": "sms-backup-restore",
    "tool": "SMS Backup & Restore",
    "tool_version": "10.26.003",
    "owner_handle": "+15555550100",
    "owner_display_name": "Me"
  },
  "conversation": {
    "chat_identifier": "+15555550101",
    "conversation_type": "individual",
    "group_title": null,
    "participants": [{ "handle": "+15555550101", "display_name": "Sam" }],
    "stats": {
      "message_count": 1,
      "attachment_count": 0,
      "first_timestamp_unix_ms": 1400773261000,
      "last_timestamp_unix_ms": 1400773261000
    }
  },
  "messages": [
    {
      "guid": "…",
      "timestamp_unix_ms": 1400773261000,
      "direction": "outgoing",
      "service": "sms",
      "message_kind": "sms",
      "sender_handle": "+15555550100",
      "sender_display_name": "Me",
      "subject": null,
      "text": "Hello",
      "attachments": [],
      "imessage": null,
      "source": {
        "android_type": 2,
        "fields": { "kind": "sms", "attrs": { "address": "+15555550101" } }
      }
    }
  ]
}
```

| Tier | Contents |
|------|----------|
| Core | typed conversation + message fields |
| `imessage` | typed Apple extensions (`IrImessage`); nested `parts` / `edits` / `tapbacks` / `app` are JSON values |
| `source` | `android_type` (`i32` or null) + vendor `fields` object |

### Identity

- Outgoing rows set `sender_handle` / `sender_display_name` from `export.owner_*` (display defaults to `"Me"` when a handle is known).
- Incoming rows use the peer identity.
- Display names are not duplicated under `source`.

### Attachments

Attachment **bytes** are never stored in JSON/JSONL (`#[serde(skip)]`). Paths + digests point at sidecar files under `attachments/`. JSON/JSONL alone is not enough to rebuild mail with media — keep the attachment directory with the export.

### Vocabulary (enums)

- `conversation_type`: `individual` \| `group`
- `service`: `sms` \| `imessage` \| `whatsapp` \| `rcs` \| `unknown`
- `message_kind`: `sms` \| `mms` \| `imessage` \| `tapback` \| `sticker_tapback` \| `announcement` \| `location_share` \| `balloon` \| `unknown`

### Serialization rules

- Optional strings / bags serialize as `null` when absent (stable keys).
- Empty `participants` / `attachments` serialize as `[]`.
- Packaging stem suffix is not part of the document (internal `packaging_stem_suffix` only).

### Conversation stats

`conversation.stats` is computed from messages at write time (`message_count`, `attachment_count`, first/last `timestamp_unix_ms`).

## JSONL layout

```text
{"schema_version":3,"export":{…},"conversation":{…}}
{"guid":"…","timestamp_unix_ms":…, …}
…
```

Line 1 is the header (includes `conversation.stats`; no `messages` array). Each following line is one `IrMessage`.

## Projectors

| Format | Writer |
|--------|--------|
| JSON | pretty-printed `ConversationDocument` |
| JSONL | header + one message per line |
| CSV | unified [`CSV_HEADERS`](../crates/message-ir/src/lib.rs) for all sources; nested bags stringified into `source_fields_json` / `parts_json` / … cells (Apple cells empty for SMS) |
| EML / MBOX | IR → `MailMessage` → [`message-mail`](../crates/message-mail/) |

## Related

- [MAIL_ARCHIVE.md](MAIL_ARCHIVE.md) — EML/MBOX packaging
- [csv-output.md](src/csv-output.md) — CSV conventions
- [EXPORTER_MATRIX.md](EXPORTER_MATRIX.md)
