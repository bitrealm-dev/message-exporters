# Exporter capability matrix

What each converter writes (and where it falls short). Marks: **yes** / **partial** / **no**.

## Shared model

All converters write **one CSV file per conversation**. Across the board:

- The peer is `chat_identifier` — there is **no** dedicated receiver-phone column
- Direction is `direction` (`incoming` / `outgoing`) — there is **no** `is_from_me` column
- Outgoing rows leave `sender_handle` blank by design (iMessage Exporter still fills `sender_display_name` with `"Me"` or `--custom-name`)
- Your own (owner) phone number is **not** written as a CSV column

## Capabilities

| | GO SMS Pro | SMS Backup & Restore | SMS Backup+ | OpenExtract | iMazing | iMessage Exporter |
|---|---|---|---|---|---|---|
| **Output** | Per-chat CSV | Per-chat CSV | Per-chat CSV | Per-chat CSV | Per-chat CSV (`__whatsapp` for WA) | Per-chat CSV (also txt/html) |
| **Peer phone** (`chat_identifier`) | yes | yes | yes (or `unknown`) | partial (name stem if unresolved) | partial (name stem if unresolved) | yes (Apple chat id) |
| **Sender phone** (`sender_handle`, incoming) | yes | yes | yes | yes | yes | yes |
| **Names** | yes (XML + contacts) | yes (XML + contacts) | yes (contacts + name-mapping) | partial (contacts critical) | yes (Contacts CSV) | yes (AddressBook / backup) |
| **Direction** | yes | yes | yes | yes (`Is From Me` / Direction) | yes (`Type`) | yes (`is_from_me` in DB) |
| **Groups** | partial (PDU MMS) | yes (MMS) | partial (flat multi-address) | no | partial (WhatsApp roster weak) | yes (full DB roster) |
| **Attachments** | partial (PDU only; XML none) | yes (MMS) | yes (archive pairing heuristic) | no (flag only) | yes | yes |
| **Media modes** (`clone`/`convert`/`compress`) | yes | yes | yes | no | yes | yes (`clone`/`basic`/`full`/`disabled`) |
| **Contacts** | optional | optional | optional | recommended | recommended | optional |
| **Owner phone CLI** | required | required | required (+ owner email) | no | no | no |

## Deficiencies

| Exporter | Main gaps |
|---|---|
| **GO SMS Pro** | Proprietary MMS `.pdu` (heuristic decode); many empty stub PDUs; `export_tool_version` unpinned; SMS attachments not in XML |
| **SMS Backup & Restore** | Call logs ignored; drafts / failed / queued skipped; encrypted ZIP not supported (unlock first) |
| **SMS Backup+** | Offline `.eml` only (no IMAP); archive attachment→message pairing is guesswork; unresolved peers → `unknown.csv` |
| **OpenExtract** | No media extraction; no groups; thin source format; name-only chats common without a good VCF |
| **iMazing** | Outgoing sender identity blank (upstream); reactions/replies are free text; WhatsApp groups lack full roster; naive dates need `--timezone` |
| **iMessage Exporter** | Outgoing `sender_handle` blank; no WhatsApp; GPL-3.0-or-later; needs Mac/`chat.db` access |

## Other dimensions

| | GO SMS Pro | SMS Backup & Restore | SMS Backup+ | OpenExtract | iMazing | iMessage Exporter |
|---|---|---|---|---|---|---|
| **WhatsApp** | no | no | no | no | yes | no |
| **`participants_json`** | no | no | no | no | no | yes |
| **Reactions / tapbacks** | no | no | no | no | free-text `reactions` | structured `tapbacks_json` |
| **Edits / replies** | no | no | no | no | raw dates / free-text | `edits_json` / thread GUIDs |
| **Source extras** | `pdu_*`, `xml_fields_json` | `subject`, `message_kind`, `xml_fields_json` | `smssync_id`, `eml_path` | `openextract_has_attachments` | vendor date / status cols | `parts_json`, `app_json`, … |
| **Timezone** | XML/PDU epoch | XML epoch | EML dates | vendor `Date` | naive + `--timezone` | DB epoch + offset |
| **Skip diagnostics** | `skipped_*.csv` (invalid address, empty PDU, no party) | counters on stderr | counters on stderr | unresolved phone count | counters on stderr | upstream logging |

## Format docs

| Exporter | Mapping / design |
|---|---|
| GO SMS Pro | [`crates/go-sms-pro-out/docs/XML_CSV_MAPPING.md`](../crates/go-sms-pro-out/docs/XML_CSV_MAPPING.md) |
| SMS Backup & Restore | [`crates/sms-backup-restore-out/docs/XML_CSV_MAPPING.md`](../crates/sms-backup-restore-out/docs/XML_CSV_MAPPING.md) |
| SMS Backup+ | [`crates/sms-backup-plus-out/docs/EML_CSV_MAPPING.md`](../crates/sms-backup-plus-out/docs/EML_CSV_MAPPING.md) |
| OpenExtract | [`crates/openextract-out/README.md`](../crates/openextract-out/README.md) |
| iMazing | [`crates/imazing-out/docs/DESIGN.md`](../crates/imazing-out/docs/DESIGN.md) |
| iMessage Exporter | [`crates/imessage-exporter/README.md`](../crates/imessage-exporter/README.md) |
