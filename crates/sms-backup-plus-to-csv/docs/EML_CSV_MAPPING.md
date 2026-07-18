# SMS Backup+ EML → CSV mapping

How flat and archive `.eml` messages map to per-conversation CSV rows written by `sms-backup-plus-to-csv`.

Deeper EML format notes: [`FORMAT.md`](FORMAT.md).

## Goal / non-goal

- **Goal:** Emit columns Backup+ can fill. Where a concept matches iMessage CSV, reuse that field name.
- **Non-goal:** A universal CSV schema shared with every exporter, or a full iMessage column skeleton with empty placeholders. Unused iMessage-only fields are omitted.

## Output

One `{chat_id}.csv` per conversation (header + one row per message after dedupe), plus MIME attachments under `attachments/`.

## EML shapes

### Flat (one SMS/MMS per file)

Typical headers: `X-smssync-type`, `X-smssync-address`, `X-smssync-date`, `X-smssync-id`, `Subject: SMS with …`.

### Archive (many messages in one file)

`Subject: SMS archive …`, body lines `YYYY-MM-DD HH:MM:SS - {Sender}` then text; sender `Me` = outgoing.

## Columns (imessage names where shared)

| CSV column | EML source |
|------------|------------|
| `chat_identifier` | Peer E.164 or `chat-group-…` |
| `conversation_type` | `individual` / `group` from address list |
| `group_title` | Derived for groups (empty for 1:1) |
| `guid` | Deterministic SHA-256 fingerprint |
| `timestamp` / `timestamp_utc` / `timestamp_display` | Flat: `X-smssync-date` / `Date`; archive: body timestamp |
| `direction` | `incoming` / `outgoing` from `X-smssync-type` or archive sender |
| `service` | Always `SMS` |
| `sender_handle` / `sender_display_name` | Empty when outgoing |
| `text` | First `text/plain` (flat) or archive body text |
| `attachments_json` | Non-text MIME parts under `attachments/` |

## Backup+-only columns

| CSV column | Source |
|------------|--------|
| `export_source` | Always `sms-backup-plus` |
| `source_kind` | `flat` or `archive` |
| `smssync_id` | `X-smssync-id` (flat only) |
| `date_ms` | Timestamp as milliseconds string |
| `contact_name` | Subject / name hint |
| `android_type` | Raw `X-smssync-type` when present |
| `eml_path` | Source `.eml` path (relative to an `--input` root when possible) |

## Deduplication

Duplicates are collapsed **while scanning** with a cover key (archive↔flat `cover_identity`):

`{chat_id}|{timestamp_ms_floored_to_second}|{0|1}|{normalized_text}`

That ignores sub-second time and `X-smssync-id`, so an archive line at `12:00:00` matches a flat with `X-smssync-date` ms inside that second. When two copies collide, **flat wins over archive** for metadata (`smssync_id`, etc.); attachments are merged by content digest so MMS media is not dropped. Otherwise the earlier timestamp wins. Rows are sorted by time before writing.

Text normalization collapses whitespace.
