# SMS Backup+ EML → common message / CSV mapping

How flat and archive `.eml` messages map into the common message and the shared CSV projector written by `sms-backup-plus-exporter` (`--format csv`).

Deeper EML format notes: [`FORMAT.md`](FORMAT.md). Shared CSV contract: [`docs/src/csv-output.md`](../../../docs/src/csv-output.md), [`message_ir::CSV_HEADERS`](../../message-ir/src/lib.rs). Common message: [`docs/src/common-message.md`](../../../docs/src/common-message.md), [`docs/COMMON_MESSAGE.md`](../../../docs/COMMON_MESSAGE.md).

## Goal / non-goal

- **Goal:** Document how Backup+ EML fields fill shared common-message / CSV cells (and the vendor bag).
- **Non-goal:** A private per-exporter CSV header, or omitting unused Apple columns. All exporters write the full [`CSV_HEADERS`](../../message-ir/src/lib.rs); Apple-only cells are empty for this source.

## Pipeline / output

Source EML → `ConversationDocument` → [`message_ir::FormatSink`](../../message-ir/src/format_sink.rs) (`--format json|jsonl|csv|eml|mbox|xml`; default `json`).

With `--format csv`: one file per conversation (header + one row per message after dedupe). MIME attachments under `attachments/` when copying/embedding. Filenames: 1:1 → `+E164.csv`; untitled groups → `group_+A_+B_….csv` (max 10 phones, then a hash). Peers with no usable phone number are written to `unknown.csv`. `--format xml` writes a single SyncTech `smses.xml`.

## EML layouts

### Flat (one SMS/MMS per file)

Typical headers: `X-smssync-type`, `X-smssync-address`, `X-smssync-date`, `X-smssync-id`, `Subject: SMS with …`.

### Archive (many messages in one file)

`Subject: SMS archive …`, body lines `YYYY-MM-DD HH:MM:SS - {Sender}` then text; sender `Me` = outgoing.

## Source → shared cells

| CSV / common-message cell | EML source |
|---------------|------------|
| `chat_identifier` | Peer E.164 or `chat-group-…` |
| `conversation_type` | `individual` / `group` from address list |
| `group_title` | Derived for groups (empty for 1:1) |
| `participants_json` | Peer handles for the conversation |
| `guid` | Deterministic SHA-256 fingerprint |
| `timestamp` / `timestamp_utc` / `timestamp_display` / `timestamp_unix_ms` | Flat: `X-smssync-date` / `Date`; archive: body timestamp |
| `direction` | `incoming` / `outgoing` from `X-smssync-type` or archive sender |
| `service` | Always `SMS` |
| `sender_handle` / `sender_display_name` | Outgoing uses export owner; incoming may use Subject / name hint |
| `text` | First `text/plain` (flat) or archive body text |
| `attachments_json` | Non-text MIME parts under `attachments/` |
| `message_kind` | `sms` or `mms` |
| `export_source` / `export_tool` / `export_tool_version` | `sms-backup-plus` / `SMS Backup+` / `1.5.11` |
| `owner_handle` / `owner_display_name` | Export owner |
| `android_type` | Raw `X-smssync-type` when present |
| `source_fields_json` | Vendor bag (below) |

Apple-only columns stay empty.

## `source_fields_json`

| Bag key | Meaning |
|---------|---------|
| `source_kind` | `flat` or `archive` |
| `smssync_id` | `X-smssync-id` when present |
| `eml_path` | Relative path to the source `.eml` |

## Deduplication

Duplicates are collapsed **while scanning** with a cover key (archive↔flat `cover_identity`):

`{chat_id}|{timestamp_ms_floored_to_second}|{0|1}|{normalized_text}`

That ignores sub-second time and `X-smssync-id`, so an archive line at `12:00:00` matches a flat with `X-smssync-date` ms inside that second. When two copies collide, **flat wins over archive** for metadata (`smssync_id`, etc.); attachments are merged by content digest so MMS media is not dropped. Otherwise the earlier timestamp wins. Rows are sorted by time before writing.

Text normalization collapses whitespace.
