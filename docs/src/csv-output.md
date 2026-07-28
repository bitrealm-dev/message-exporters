# CSV output

All converters write **one CSV file per conversation**, plus an `attachments/` directory when media is copied.

CSV is projected from [canonical IR](../MESSAGE_IR.md) (`schema_version` 3) via `message_ir::write_format`. A **mail archive** path is specified in [`MAIL_ARCHIVE.md`](../MAIL_ARCHIVE.md).

## Shared conventions

- Peer identity is `chat_identifier` (there is no separate receiver-phone column).
- Direction is `direction` (`incoming` / `outgoing`).
- Outgoing rows fill `sender_handle` / `sender_display_name` from `owner_handle` / `owner_display_name` (display defaults to `"Me"` when a handle is known).
- Every row includes `export_source`, `export_tool`, `export_tool_version`, and owner columns.
- All exporters share one header set ([`CSV_HEADERS`](../../crates/message-ir/src/lib.rs)). Apple-only cells are empty for non-iMessage sources.

## Column contract

| Column | Notes |
|--------|--------|
| `chat_identifier` | Conversation id |
| `conversation_type` | `individual` / `group` |
| `group_title` | Empty when none |
| `participants_json` | `[{handle, display_name}, …]` |
| `guid` | Stable message id |
| `timestamp` / `timestamp_utc` / `timestamp_display` | Local RFC3339, UTC RFC3339, human display |
| `timestamp_unix_ms` | Unix epoch milliseconds (replaces legacy `date_ms`) |
| `direction` | `incoming` / `outgoing` |
| `service` | `sms` / `imessage` / `whatsapp` / `rcs` / `unknown` |
| `sender_handle` / `sender_display_name` | Peer or owner (outgoing) |
| `subject` / `text` | Message body fields |
| `attachments_json` | Array of `{path, original_name, mime_type, digest_sha256?, …}` |
| `message_kind` | `sms` / `mms` / `imessage` / `tapback` / … |
| `export_source` / `export_tool` / `export_tool_version` | Provenance |
| `owner_handle` / `owner_display_name` | Export owner identity |
| `android_type` | Android Telephony type as integer string, else empty |
| `source_fields_json` | Vendor leftover object (replaces `xml_fields_json`) |
| `read_receipt` … `app_json` | iMessage extensions; empty / `null` JSON for SMS |

Removed legacy columns: `date_ms`, `contact_name`, `xml_fields_json`.

## Attachments / media modes

Converters that write `attachments/` support `--media-mode`:

| Mode | Behavior |
|------|----------|
| `disabled` | No media files |
| `clone` (default) | Copy originals |
| `convert` | Standardize to `.jpg` / `.mp4` / `.mp3` (needs ffmpeg) |
| `compress` | Re-encode with optional max resolution / fps / min size (needs ffmpeg) |

Details: [`crates/message-media`](https://github.com/bitrealm-dev/message-exporters/tree/main/crates/message-media).

## Obfuscate

Add `--obfuscate` (optional `--obfuscate-seed` with exactly 8 hex characters) to rewrite names, numbers, message text (same length), and media into a shareable structure without PII. See [`crates/message-obfuscate`](https://github.com/bitrealm-dev/message-exporters/tree/main/crates/message-obfuscate).

## Related

- [MESSAGE_IR.md](../MESSAGE_IR.md)
- [exporter overview](exporters/overview.md)
