# GO SMS Pro XML → CSV mapping

How `gosms_sys*.xml` `<SMS>` elements map to per-conversation CSV rows written by `go-sms-pro-exporter`.

PDU (`I_*.pdu`) rows use the same CSV columns; see [PDU notes](#pdu-rows) at the end.

## Goal / non-goal

- **Goal:** Emit columns GO SMS Pro can fill. Where a concept matches iMessage CSV, reuse that field name.
- **Non-goal:** A universal CSV schema shared with every exporter, or a full iMessage column skeleton with empty placeholders.

## XML shape

```xml
<GoSms>
  <SMSCount>…</SMSCount>
  <SMS>
    <address>…</address>
    <contactName>…</contactName>
    <date>…</date>          <!-- Unix ms -->
    <type>1|2</type>        <!-- 1 = inbox, 2 = sent -->
    <body>…</body>
    <!-- any other Telephony-style children are kept in source_fields_json -->
  </SMS>
</GoSms>
```

Each `<SMS>` becomes one CSV data row. The `chat_identifier` column holds the peer’s E.164 handle. On disk, 1:1 files are `+14075551234.csv`; untitled groups are `group_+A_+B_….csv` (max 10 phones, then a hash).

## Known XML children → CSV

| XML child | CSV column(s) | Notes |
|-----------|---------------|--------|
| `<address>` | `chat_identifier`, `sender_handle` | Digits sanitized then E.164. For sent (`type=2`), address is the peer (not the sender). For received (`type=1`), address is also `sender_handle` unless Google Voice voicemail parsing overrides it from `<body>`. |
| `<contactName>` | `sender_display_name` | Display name filled for incoming when present. |
| `<date>` | `timestamp_unix_ms`, `timestamp`, `timestamp_utc`, `timestamp_display` | Raw ms string in `timestamp_unix_ms`. Converted to local/UTC RFC3339 and a human display string. |
| `<type>` | `android_type`, `direction` | `1` → `incoming`, `2` → `outgoing`. Other values are skipped. |
| `<body>` | `text` | GO SMS emoji codes (e.g. `+g1f602`) decoded to Unicode. |
| *(all children)* | `source_fields_json` | Full map of every child element name → text (includes the five above plus extras such as `read`, `status`, `date_sent`, …). |

## Columns (imessage names where shared)

| CSV column | Source |
|------------|--------|
| `conversation_type` | Always `individual` for XML SMS; `group` from PDU PLMN lists |
| `group_title` | Derived for PDU groups; empty for XML |
| `guid` | SHA-256 of chat id + local timestamp + direction + text + attachment digests |
| `service` | Always `SMS` |
| `sender_handle` / `sender_display_name` | Empty when `direction=outgoing` |
| `attachments_json` | `[]` for XML; media paths for PDU |

## SMS-Pro-only columns

| CSV column | Meaning |
|------------|---------|
| `export_source` | Always `go-sms-pro` |
| `export_tool` | Always `GO SMS Pro` |
| `export_tool_version` | Empty until a target app version is pinned |
| `android_type` | Raw `<type>` (`1`/`2`); empty for PDU |
| `timestamp_unix_ms` | From `<date>` ms (also on shared header) |
| `source_fields_json` | Vendor bag: XML children and/or `source_kind` / `pdu_*` keys |

## Skip counters (CLI summary)

Printed only when non-zero:

| Label | Meaning |
|-------|---------|
| `skipped bad date` | XML `<date>` was not a number |
| `skipped date range` | Message outside `--start-date` / `--end-date` |
| `skipped bad type` | XML `<type>` was not `1` (inbox) or `2` (sent) |
| `skipped invalid address` | XML SMS with no usable phone digits in `<address>` (empty, under 4 digits, email-like, junk). 4–6 digit short codes (e.g. AT&T `7535`) are kept. Google Voice voicemail can still export if the caller is parsed from `<body>`. Full list: `skipped_invalid_address.csv`; first 10 also printed on stderr. |
| `skipped empty pdu` | Hollow PDU stub with no participants, From/To, body, or attachments (common GO SMS Pro placeholder is only `application/smil` + null). Full list: `skipped_empty_pdu.csv`. |
| `skipped no party` | Non-empty PDU classified as non-group (`< 3` unique participants) where every decoded number was empty or the owner (`--owner-phone`). Full list: `skipped_no_party.csv` (`pdu_filename`, `participants`, `is_sent`, `has_from`, `has_to`); first 10 also printed on stderr. |
| `skipped bad PDU` | PDU filename/timestamp could not be parsed |

## PDU rows

MMS from `I_<unix>_*.pdu` files use the same header. Differences:

| CSV column | PDU behavior |
|------------|--------------|
| `source_kind` | `pdu` |
| `chat_identifier` / `conversation_type` / `group_title` | From PLMN participants; groups use `chat-group-…` ids |
| `timestamp*` | MMS `Date` header when present; else filename `I_<unix>_` (seconds). Filename still required to accept the file. |
| `text` | Content-Location text parts / multipart `text/*` (emoji-decoded); marker/`</smil>` fallback if needed |
| `attachments_json` | Named/typed media parts, else magic-byte splits under `attachments/` |
| `android_type`, `timestamp_unix_ms`, `source_fields_json` | Empty |
| `pdu_filename` | Source PDU basename |
| `pdu_fields_json` | Optional MMS headers (see below); empty for XML |
| `pdu_decode` | `structured` / `mixed` / `heuristic` confidence for body, attachments, and direction; empty for XML |

`pdu_fields_json` keys when present: `subject`, `message_id`, `message_type`, `mms_version`, `message_size`, `message_class`, `transaction_id`, `priority`, `delivery_report`, `read_report`, `report_allowed`, `delivery_time`, `expiry`, `status`, `response_status`, `response_text`, `sender_visibility`, `bcc` (comma-joined), plus `app:<name>` for non-well-known MMS application headers.

`message_size` is the WAP-209 Message-Size long-integer (advisory octets). GO SMS Pro `0x8e` + `filename\0` named parts are unrelated and are not decoded as Message-Size.

### MMS parse path

1. **Structured decode** (`mms_enc`): WAP-209 headers (From/To/Cc/Bcc/Date/Subject/Status/…) + Content-Location named parts + mid-file / offset-0 multipart (part Content-ID, Content-Disposition/Filename, Content-Type Name/Filename/Start/Type/Start-info). Direction from decoded address roles; body from named parts, multipart text (including SMIL `cid:` → Content-ID), or Subject; attachments from named/typed parts and SMIL `src` / `cid:` / filename.
2. **Heuristic fallback**: PLMN regex for raw address lists, legacy `text_*.txt` markers / `</smil>` printable tails, and magic-byte attachment splits — only when the structured path left that field empty.

Algorithm reference: OMA WAP-209 / WAP-230 and the decode concepts in [python-messaging](https://github.com/pmarti/python-messaging) `messaging/mms` (not a dependency; not copied).
