# SMS Backup & Restore XML → CSV mapping

How SyncTech `<sms>` / `<mms>` elements map to per-conversation CSV rows written by `sms-backup-restore-exporter`.

Attribute meanings (SyncTech reference): [FIELDS.md](FIELDS.md).

## Goal / non-goal

- **Goal:** Emit columns SBR can fill. Where a concept matches iMessage CSV, reuse that field name.
- **Non-goal:** A universal CSV schema shared with every exporter, or a full iMessage column skeleton with empty placeholders.

## Output

One CSV file per conversation (header + one row per message), plus decoded MMS media under `attachments/`. Filenames: 1:1 → `+15555550101.csv`; untitled groups → `group_+A_+B_….csv` (max 10 phones, then a hash). The `chat_identifier` column may still use `chat-group-…` for groups.

## Columns (imessage names where shared)

| CSV column | SMS / MMS source |
|------------|------------------|
| `chat_identifier` | Peer E.164, or `chat-group-…` for groups |
| `conversation_type` | `individual` / `group` |
| `group_title` | Derived for groups; empty for 1:1 |
| `guid` | Deterministic SHA-256 fingerprint |
| `timestamp` / `timestamp_utc` / `timestamp_display` | From `date` (Java ms UTC) |
| `direction` | `incoming` / `outgoing` from SMS `type` or MMS `msg_box` / From addr |
| `service` | Always `SMS` |
| `sender_handle` / `sender_display_name` | Empty when outgoing |
| `subject` | SMS `subject`, or MMS `sub` |
| `text` | SMS `body`, or MMS text/plain parts (HTML entities decoded) |
| `attachments_json` | Extracted MMS media paths |

## SBR-only columns

| CSV column | Meaning |
|------------|---------|
| `export_source` | Always `sms-backup-restore` |
| `export_tool` | Always `SMS Backup & Restore` |
| `export_tool_version` | Always `10.26.003` (targeted Android app version) |
| `message_kind` | `sms` or `mms` |
| `timestamp_unix_ms` | Epoch ms from `date` |
| `android_type` | SMS `type`, or MMS `msg_box` |
| `source_fields_json` | Full fidelity JSON (below) |
| `owner_handle` / `owner_display_name` | Export owner; also used for outgoing `sender_*` |

## How the exporter uses SMS fields

- `address` → `chat_identifier` / participant handle (after phone normalization)
- `date` → `timestamp*` columns and `timestamp_unix_ms` (invalid or missing dates are skipped)
- `type` `1` / `2` → `direction` incoming / outgoing; other types are skipped; raw value in `android_type`
- `body` → `text` (HTML entities decoded)
- `subject` → `subject` when present
- `contact_name` → `sender_display_name` for incoming (not a separate CSV column)
- **Every** `<sms>` attribute → `source_fields_json.attrs`

Example: `<sms address="+15555550101" date="1400773261000" type="1" body="hello &amp; hi" contact_name="Sam" />` becomes an incoming CSV row with `chat_identifier=+15555550101` (file `+15555550101.csv`) and text `hello & hi`.

## How the exporter uses MMS fields

- `date` → `timestamp*` / `timestamp_unix_ms` (bad dates skipped)
- `msg_box` `2` → outgoing; `1` → incoming (From addr `type="137"` sets the sender when present); raw `msg_box` in `android_type`
- `msg_box` `3` (draft) and `4` (outbox) are skipped and counted as `skipped_draft_or_outbox` (not `skipped_unknown_type`, which is for unknown SMS `type` only)
- `sub` → `subject`
- `address` plus `<addr>` list → participants; one other person is a 1:1 chat, more than one is a group
- `text/plain` parts → `text`; SMIL (`application/smil`) controls text/image order when present
- Non-text `data` → files under `attachments/` and `attachments_json`; in `source_fields_json.parts`, `data` is replaced with `data_len` + `data_sha256`
- Every `<mms>` / `<part>` / `<addr>` attribute → `source_fields_json`
- Empty participant lists and undecodable attachment base64 are skipped and counted in the run report

Example group address string: `+15555550101~+15555550102` with two From/To addrs becomes a group chat titled from those two numbers.

## `source_fields_json`

### SMS

```json
{ "kind": "sms", "attrs": { /* every <sms> attribute */ } }
```

### MMS

```json
{
  "kind": "mms",
  "attrs": { /* every <mms> attribute */ },
  "parts": [ { /* every <part> attribute */ } ],
  "addrs": [ { /* every <addr> attribute */ } ]
}
```

For each `<part>` that has a `data` attribute, CSV stores `data_len` and `data_sha256` of the **decoded** bytes and **omits** the base64 `data` string (binaries live under `attachments/`). Other part attributes (`seq`, `ct`, `name`, `cl`, `chset`, `text`, …) are kept as-is.

## Reverse: IR → XML

Exporters can write a SyncTech `smses.xml` via `--format xml` ([`docs/SBR_XML.md`](../../../docs/SBR_XML.md)). When `source.fields` still holds the `kind`/`attrs`/`parts`/`addrs` bag from this importer, those attributes are preferred. Otherwise SMS/MMS elements are synthesized from IR core fields. iMessage-only IR is lossy (Apple bags dropped).

## Not exported

`<call>` / call-log rows in the same backup file are ignored.
