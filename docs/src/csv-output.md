# CSV output

All converters write **one CSV file per conversation**, plus an `attachments/` directory when media is copied.

A **mail archive** path (folder of `.eml` per conversation, `X-ME-*` headers) is specified in [`MAIL_ARCHIVE.md`](../MAIL_ARCHIVE.md) (`message-mail` via `message-ir`). This page describes the current CSV output only.

## Shared conventions

- Peer identity is `chat_identifier` (there is no separate receiver-phone column).
- Direction is `direction` (`incoming` / `outgoing`).
- Outgoing rows leave `sender_handle` blank by design.
- Your own (owner) phone number is not written as a CSV column.
- Every row includes `export_source`, `export_tool`, and `export_tool_version` for downstream import.

See the [exporter overview](exporters/overview.md) for a full capability matrix.

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
