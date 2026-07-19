# GO SMS Pro XML → CSV mapping

How `gosms_sys*.xml` `<SMS>` elements map to per-conversation CSV rows written by `go-sms-pro-out`.

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
    <!-- any other Telephony-style children are kept in xml_fields_json -->
  </SMS>
</GoSms>
```

Each `<SMS>` becomes one CSV data row. The `chat_identifier` column holds the peer’s E.164 handle. On disk, conversation files use a sanitized stem (`+` → `_`), for example `+14075551234` → `_14075551234.csv`.

## Known XML children → CSV

| XML child | CSV column(s) | Notes |
|-----------|---------------|--------|
| `<address>` | `chat_identifier`, `sender_handle` | Digits sanitized then E.164. For sent (`type=2`), address is the peer (not the sender). For received (`type=1`), address is also `sender_handle` unless Google Voice voicemail parsing overrides it from `<body>`. |
| `<contactName>` | `contact_name`, `sender_display_name` | Raw string in `contact_name`. Display name filled for incoming when present. |
| `<date>` | `date_ms`, `timestamp`, `timestamp_utc`, `timestamp_display` | Raw ms string in `date_ms`. Converted to local/UTC RFC3339 and a human display string. |
| `<type>` | `android_type`, `direction` | `1` → `incoming`, `2` → `outgoing`. Other values are skipped. |
| `<body>` | `text` | GO SMS emoji codes (e.g. `+g1f602`) decoded to Unicode. |
| *(all children)* | `xml_fields_json` | Full map of every child element name → text (includes the five above plus extras such as `read`, `status`, `date_sent`, …). |

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
| `source_kind` | `xml` or `pdu` |
| `android_type` | Raw `<type>` (`1`/`2`); empty for PDU |
| `date_ms` | Raw `<date>` ms; empty for PDU |
| `contact_name` | Raw `<contactName>`; empty for PDU |
| `pdu_filename` | PDU basename; empty for XML |
| `xml_fields_json` | All `<SMS>` children as JSON object; empty for PDU |

## PDU rows

MMS from `I_<unix>_*.pdu` files use the same header. Differences:

| CSV column | PDU behavior |
|------------|--------------|
| `source_kind` | `pdu` |
| `chat_identifier` / `conversation_type` / `group_title` | From PLMN participants; groups use `chat-group-…` ids |
| `timestamp*` | From PDU filename timestamp (seconds) |
| `text` | Extracted WAP text body (emoji-decoded) |
| `attachments_json` | Extracted media under `attachments/` |
| `android_type`, `date_ms`, `contact_name`, `xml_fields_json` | Empty |
| `pdu_filename` | Source PDU basename |
