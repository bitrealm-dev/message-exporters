# SMS Backup & Restore XML → common message / CSV mapping

How SyncTech `<sms>` / `<mms>` elements map into the common message (`ConversationDocument`) and the shared CSV projector written by `sms-backup-restore-exporter` (`--format csv`).

Attribute meanings (SyncTech **input** reference): [FIELDS.md](FIELDS.md). Shared CSV contract: [`docs/src/csv-output.md`](../../../docs/src/csv-output.md), [`message_ir::CSV_HEADERS`](../../message-ir/src/lib.rs). Common message: [`docs/src/common-message.md`](../../../docs/src/common-message.md), [`docs/COMMON_MESSAGE.md`](../../../docs/COMMON_MESSAGE.md).

## Goal / non-goal

- **Goal:** Document how SyncTech fields fill shared common-message / CSV cells (and the vendor bag).
- **Non-goal:** A private per-exporter CSV header. All exporters use one CSV header; Apple-only cells are empty for this source.

## Pipeline / output

Source XML → `ConversationDocument` → [`message_ir::FormatSink`](../../message-ir/src/format_sink.rs) (`--format json|jsonl|csv|eml|mbox|xml`; default `json`).

With `--format csv`: one file per conversation (header + one row per message). Decoded MMS media under `attachments/` when copying/embedding. Filenames: 1:1 → `+E164.csv`; untitled groups → `group_+A_+B_….csv` (max 10 phones, then a hash). The `chat_identifier` cell may still use `chat-group-…` for groups. `--format xml` writes a single SyncTech `smses.xml` ([`docs/SBR_XML.md`](../../../docs/SBR_XML.md)).

## Source → shared cells

| CSV / common-message cell | SMS / MMS source |
|---------------|------------------|
| `chat_identifier` | Peer E.164, or `chat-group-…` for groups |
| `conversation_type` | `individual` / `group` |
| `group_title` | Derived for groups; empty for 1:1 |
| `participants_json` | Peer handles from SMS address / MMS `<addr>` list |
| `guid` | Deterministic SHA-256 fingerprint |
| `timestamp` / `timestamp_utc` / `timestamp_display` / `timestamp_unix_ms` | From `date` (Unix epoch milliseconds, UTC) |
| `direction` | `incoming` / `outgoing` from SMS `type` or MMS `msg_box` / From addr |
| `service` | Always `SMS` |
| `sender_handle` / `sender_display_name` | Incoming peer; outgoing uses export owner (`owner_*`) |
| `subject` | SMS `subject`, or MMS `sub` |
| `text` | SMS `body`, or MMS text/plain parts (HTML entities decoded) |
| `attachments_json` | Extracted MMS media paths |
| `message_kind` | `sms` or `mms` |
| `export_source` / `export_tool` / `export_tool_version` | `sms-backup-restore` / `SMS Backup & Restore` / `10.26.003` |
| `owner_handle` / `owner_display_name` | Export owner |
| `android_type` | SMS `type`, or MMS `msg_box` |
| `source_fields_json` | Full fidelity JSON (below) |

Apple-only columns (`parts_json`, tapbacks, balloons, …) stay empty.

## How the exporter uses SMS fields

- `address` → `chat_identifier` / participant handle (after phone normalization)
- `date` → `timestamp*` and `timestamp_unix_ms` (invalid or missing dates are skipped)
- `type` `1` / `2` → `direction` incoming / outgoing; other types are skipped; raw value in `android_type`
- `body` → `text` (HTML entities decoded)
- `subject` → `subject` when present
- `contact_name` → `sender_display_name` for incoming (not a separate CSV column)
- **Every** `<sms>` attribute → `source_fields_json.attrs`

Example: `<sms address="+15555550101" date="1400773261000" type="1" body="hello &amp; hi" contact_name="Sam" />` becomes an incoming row with `chat_identifier=+15555550101` and text `hello & hi`.

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

For each `<part>` that has a `data` attribute, the bag stores `data_len` and `data_sha256` of the **decoded** bytes and **omits** the base64 `data` string (binaries live under `attachments/` or are embedded for mail/Xml). Other part attributes (`seq`, `ct`, `name`, `cl`, `chset`, `text`, …) are kept as-is.

## Reverse: common message → XML

Exporters can write a SyncTech `smses.xml` via `--format xml` ([`docs/SBR_XML.md`](../../../docs/SBR_XML.md)). When `source.fields` still holds the `kind`/`attrs`/`parts`/`addrs` bag from this importer, those attributes are preferred. Otherwise SMS/MMS elements are synthesized from common-message core fields. iMessage-only common messages are lossy (Apple bags dropped).

## Not exported

`<call>` / call-log rows in the same backup file are ignored (no call messages in any output format).
