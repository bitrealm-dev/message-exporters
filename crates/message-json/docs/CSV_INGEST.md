# CSV → NDJSON ingest contract

CSV exporters in this workspace do **not** share one universal column set. Each source has a mapping file under [`crates/csv-ingest/mappings/`](../../csv-ingest/mappings/) that names which CSV columns feed which JSON fields. Each mapping TOML includes `exporter_version` (the exporter / upstream library version the mapping was written against, e.g. `imessage-database` `4.2.0`).

The converter ([`csv-ingest`](../../csv-ingest)) writes **vault** NDJSON (`schema: "vault"`, `schema_version: 1`). That format holds every message field the vault understands; sources leave unused rich fields empty/omitted. It is **not** an iMessage-only schema — `service` carries SMS vs iMessage vs other channels.

Hand-written examples and minimum-field tables: [`crates/csv-ingest/samples/vault/`](../../csv-ingest/samples/vault/).

## Pipeline

```
source CSV  +  mapping.toml  →  csv-ingest  →  vault NDJSON  →  vault import
                                                         ↘  future REST / NDJSON stream
```

## Conversation header (first line)

Same as [`message_json::vault::ConversationRecord`](../src/vault.rs):

| Field | Required | Notes |
|-------|----------|--------|
| `record` | yes | `"conversation"` |
| `schema` | yes | `"vault"` |
| `schema_version` | yes | `1` |
| `chat_identifier` | yes | From CSV |
| `service` | yes | CSV `service` or mapping `default_service` (`SMS`, `iMessage`, …) |
| `conversation_type` | yes | `individual` / `group` |
| `group_title` | no | |
| `participants` | yes | Built from peer + senders / `participants_json` |
| `exported_at` | no | Converter sets UTC now |

Provenance for vault rows is the ingest **source id** (staging / config), not a field on every conversation line. Mapping `export_source` / CSV `export_source` identify which exporter produced the CSV.

## Message fields

### Core (all sources)

| Field | Required | CSV → JSON |
|-------|----------|------------|
| `record` | yes | `"message"` |
| `guid` | yes* | Mapped `guid`, or generated if empty |
| `timestamp` | yes† | `timestamp` |
| `timestamp_utc` | no | `timestamp_utc` |
| `is_from_me` | yes | From `direction` (`outgoing` → true) |
| `sender` | no | `sender_handle` when incoming |
| `service` | no | `service` or default |
| `subject` | no | `subject` |
| `text` | yes‡ | `text` |
| `attachments` | yes‡ | Parsed from `attachments_json` |

\* If the CSV guid cell is empty, `csv-ingest` generates a stable SHA-256 hex over `chat_identifier \| timestamp \| is_from_me \| text \| attachment paths`.  
† Or `timestamp_utc` if `timestamp` empty.  
‡ At least one of non-empty `text`, non-empty `attachments`, or announcement (below).

### Rich (usually iMessage; omit when unused)

| Field | CSV → JSON |
|-------|------------|
| `tapbacks` | `tapbacks_json` |
| `is_reply`, `thread_originator_guid`, `thread_originator_part`, `num_replies` | matching columns |
| `is_announcement`, `announcement` | matching columns |
| `read_receipt`, `is_deleted`, `send_effect`, `shared_location` | matching columns |
| `parts`, `edits`, `app` | `parts_json`, `edits_json`, `app_json` |

### Never mapped (exporter-only CSV columns)

`export_source`, `timestamp_display`, `source_kind`, `android_type`, `date_ms`, `contact_name`, `pdu_filename`, `xml_fields_json`, `smssync_id`, `eml_path`, `message_kind`, and similar debug/provenance cells stay out of vault JSON.

## Minimum CSV columns (after mapping)

A row is rejected unless the mapping can produce:

1. `chat_identifier`
2. `timestamp` or `timestamp_utc`
3. `direction` (for `is_from_me`)
4. `text` and/or attachments, **or** `is_announcement` + `announcement`

## Future HTTP ingest

See [`crates/csv-ingest/README.md`](../../csv-ingest/README.md#future-http-ingest). The vault NDJSON this tool writes is the intended request body.
