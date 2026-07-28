# Exporter capability matrix

What each converter writes (and where it falls short). Marks: **yes** / **partial** / **no**.

## Shared model

All converters write **one CSV file per conversation**. Across the board:

- The peer is `chat_identifier` — there is **no** dedicated receiver-phone column
- Direction is `direction` (`incoming` / `outgoing`) — there is **no** `is_from_me` column
- Outgoing rows leave `sender_handle` blank by design (iMessage Exporter still fills `sender_display_name` with `"Me"` or `--custom-name`)
- Your own (owner) phone number is **not** written as a CSV column

## Capabilities

| | GO SMS Pro | SMS Backup & Restore | SMS Backup+ | OpenExtract | iMazing | WhatsApp | iMessage Exporter |
|---|---|---|---|---|---|---|---|
| **Output** | Per-chat CSV / EML / MBOX | Per-chat CSV / EML / MBOX / **JSON IR** | Per-chat CSV / EML / MBOX | Per-chat CSV / EML / MBOX | Per-chat CSV / EML / MBOX (`__whatsapp` for WA) | Per-chat CSV / EML / MBOX (`__whatsapp`) | Per-chat CSV (also txt/html); EML/MBOX via `imessage-mail-exporter` |
| **Peer phone** (`chat_identifier`) | yes | yes | yes (or `unknown`) | partial (name stem if unresolved) | partial (name stem if unresolved) | yes (JID → E.164) | yes (Apple chat id) |
| **Sender phone** (`sender_handle`, incoming) | yes | yes | yes | yes | yes | yes (groups via sender JID) | yes |
| **Names** | yes (XML + contacts) | yes (XML + contacts) | yes (contacts + name-mapping) | partial (contacts critical) | yes (Contacts CSV) | yes (`wa.db` via wtsexporter) | yes (AddressBook / backup) |
| **Direction** | yes | yes | yes | yes (`Is From Me` / Direction) | yes (`Type`) | yes (`from_me`) | yes (`is_from_me` in DB) |
| **Groups** | partial (PDU MMS) | yes (MMS) | partial (flat multi-address) | no | partial (WhatsApp roster weak) | yes (title + sender phones) | yes (full DB roster) |
| **Attachments** | partial (PDU only; XML none) | yes (MMS) | yes (archive pairing heuristic) | no (flag only) | yes | yes (media paths via wtsexporter) | yes |
| **Media modes** (`clone`/`convert`/`compress`) | yes | yes | yes | no | yes | yes | yes (`clone`/`basic`/`full`/`disabled`) |
| **Contacts** | optional | optional | optional | recommended | recommended | via `--wa` / wtsexporter | optional |
| **Owner phone CLI** | required | required | required (+ owner email) | no | no | no | no |

## Deficiencies

| Exporter | Main gaps |
|---|---|
| **GO SMS Pro** | Proprietary MMS `.pdu` (heuristic decode); many empty stub PDUs; `export_tool_version` unpinned; SMS attachments not in XML |
| **SMS Backup & Restore** | Call logs ignored; drafts / failed / queued skipped; encrypted ZIP not supported (unlock first) |
| **SMS Backup+** | Offline `.eml` only (no IMAP); archive attachment→message pairing is guesswork; unresolved peers → `unknown.csv` |
| **OpenExtract** | No media extraction; no groups; thin source format; name-only chats common without a good VCF |
| **iMazing** | Outgoing sender identity blank (upstream); reactions/replies are free text; WhatsApp groups lack full roster; naive dates need `--timezone` |
| **WhatsApp** | Requires external `wtsexporter` (pip or bundled binary); LID / non-phone JIDs stay raw; full group roster depends on upstream JSON |
| **iMessage Exporter** | Outgoing `sender_handle` blank; no WhatsApp; GPL-3.0-or-later; needs Mac/`chat.db` access |

## Other dimensions

| | GO SMS Pro | SMS Backup & Restore | SMS Backup+ | OpenExtract | iMazing | WhatsApp | iMessage Exporter |
|---|---|---|---|---|---|---|---|
| **WhatsApp** | no | no | no | no | yes (CSV) | yes (native DB) | no |
| **`participants_json`** | no | no | no | no | no | no | yes |
| **Reactions / tapbacks** | no | no | no | no | free-text `reactions` | `whatsapp_reactions_json` | structured `tapbacks_json` |
| **Edits / replies** | no | no | no | no | raw dates / free-text | `whatsapp_reply_json` | `edits_json` / thread GUIDs |
| **Source extras** | `pdu_*`, `xml_fields_json` | `subject`, `message_kind`, `xml_fields_json` | `smssync_id`, `eml_path` | `openextract_has_attachments` | vendor date / status cols | `whatsapp_jid`, `whatsapp_key_id` | `parts_json`, `app_json`, … |
| **Timezone** | XML/PDU epoch | XML epoch | EML dates | vendor `Date` | naive + `--timezone` | epoch from wtsexporter | DB epoch + offset |
| **Skip diagnostics** | `skipped_*.csv` (invalid address, empty PDU, no party) | counters on stderr | counters on stderr | unresolved phone count | counters on stderr | counters on stderr | upstream logging |

## Format docs

| Exporter | Mapping / design |
|---|---|
| GO SMS Pro | [`crates/go-sms-pro-exporter/docs/XML_CSV_MAPPING.md`](../crates/go-sms-pro-exporter/docs/XML_CSV_MAPPING.md) |
| SMS Backup & Restore | [`crates/sms-backup-restore-exporter/docs/XML_CSV_MAPPING.md`](../crates/sms-backup-restore-exporter/docs/XML_CSV_MAPPING.md) |
| SMS Backup+ | [`crates/sms-backup-plus-exporter/docs/EML_CSV_MAPPING.md`](../crates/sms-backup-plus-exporter/docs/EML_CSV_MAPPING.md) |
| OpenExtract | [`crates/openextract-exporter/README.md`](../crates/openextract-exporter/README.md) |
| iMazing | [`crates/imazing-exporter/docs/DESIGN.md`](../crates/imazing-exporter/docs/DESIGN.md) |
| WhatsApp | [`crates/whatsapp-exporter/README.md`](../crates/whatsapp-exporter/README.md) |
| iMessage Exporter | [`crates/imessage-exporter/README.md`](../crates/imessage-exporter/README.md) |
| iMessage mail (EML) | [`crates/imessage-mail-exporter/`](../crates/imessage-mail-exporter/) |

**Canonical IR:** [`MESSAGE_IR.md`](MESSAGE_IR.md) (`message-ir`). **SMS Backup & Restore** parses to IR then projects CSV/EML/MBOX/JSON. Other exporters still emit formats directly (JSON not yet). Mail packaging: [`MAIL_ARCHIVE.md`](MAIL_ARCHIVE.md). CSV remains the default. Convert/compress/obfuscate stay CSV-only.
