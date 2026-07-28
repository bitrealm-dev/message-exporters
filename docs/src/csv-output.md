# CSV columns

CSV exports write one spreadsheet file per conversation and an `attachments/` directory when media is copied. Use this page when inspecting or importing CSV rows.

Conversation and export identity live on every data row (there is no separate header sidecar). Re-import reads those fields from the first row.

## Conventions

- Peer identity is `chat_identifier` (no separate receiver-phone column).
- Direction is `direction`: `incoming` or `outgoing`.
- Outgoing rows fill `sender_handle` / `sender_display_name` from the export owner (display defaults to `Me` when a handle is known).
- Every row includes `export_source`, `export_tool`, `export_tool_version`, and owner columns.
- All backup sources share one header set. Apple-only cells stay empty for non-iMessage sources.

## Nested JSON cells

Columns such as `source_fields_json`, `parts_json`, `edits_json`, `tapbacks_json`, and `app_json`:

- **Absent** → empty string (never the literal word `null`)
- **Present** → compact JSON

Booleans (`is_deleted`, `is_reply`, `is_announcement`) are always `true` or `false` as text.

`parts_json` is written only from the iMessage bag. A single plain `run` / `text` part that merely duplicates the `text` column is omitted; multi-part or richer bodies still write `parts_json`.

## Column list

| Column | Meaning |
|--------|---------|
| `chat_identifier` | Conversation id |
| `conversation_type` | `individual` or `group` |
| `group_title` | Empty when none |
| `participants_json` | `[{handle, display_name}, …]` |
| `guid` | Stable message id |
| `timestamp` / `timestamp_utc` / `timestamp_display` | Local RFC3339, UTC RFC3339, human display |
| `timestamp_unix_ms` | Unix epoch milliseconds |
| `direction` | `incoming` / `outgoing` |
| `service` | `sms` / `imessage` / `whatsapp` / `rcs` / `unknown` |
| `sender_handle` / `sender_display_name` | Peer or owner (outgoing) |
| `subject` / `text` | Message body fields |
| `attachments_json` | Array of attachment objects (`path`, `original_name`, `mime_type`, digests when present) |
| `message_kind` | `sms` / `mms` / `imessage` / `tapback` / … |
| `export_source` / `export_tool` / `export_tool_version` | Provenance |
| `owner_handle` / `owner_display_name` | Export owner identity |
| `android_type` | Android Telephony type as an integer string, else empty |
| `source_fields_json` | Vendor leftover object |
| `read_receipt` … `tapback_action` | iMessage extensions; empty for SMS |

Legacy columns `date_ms`, `contact_name`, and `xml_fields_json` are not written.

## Related

- [Choose an output format](formats.md)
- [Attachments and privacy](attachments-privacy.md)
- Common message schema (contributors): [MESSAGE_IR.md](../MESSAGE_IR.md)
- End-user overview: [Common message](common-message.md)
