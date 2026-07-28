# Fields in SMS Backup & Restore XML

SyncTech’s **input** attribute reference for the XML backup format. How this exporter maps those attributes into IR / shared CSV: [XML_CSV_MAPPING.md](XML_CSV_MAPPING.md). Canonical IR: [`docs/MESSAGE_IR.md`](../../../docs/MESSAGE_IR.md).

Source: SyncTech’s [Fields in XML backup files](https://www.synctech.com.au/sms-backup-restore/fields-in-xml-backup-files/). Related SyncTech links:

- [Sample XML](https://synctech.com.au/wp-content/uploads/2018/01/sms-sample.xml_.txt)
- [XSD schema](https://synctech.com.au/wp-content/uploads/2018/01/sms.xsd_.txt)
- [Date field FAQ](https://www.synctech.com.au/faqs/what-is-that-number-the-date-field-in-the-backup-file/) — dates are Java epoch milliseconds in UTC (for example `1400773261000` → 2014-05-22)

## File shape

Root element: `<smses>` (current) or `<allsms>` (legacy).

Child message elements:

- `<sms>` — plain text SMS
- `<mms>` — MMS with nested parts and addresses

Call logs use `<calls>` / `<call>`. They are documented below for reference. This exporter ignores call records for all IR projectors (CSV, mail, JSON, Xml).

Writing SyncTech XML from IR (`--format xml` → `smses.xml`) is documented in [`docs/SBR_XML.md`](../../../docs/SBR_XML.md).

Field values are generally copied as-is from the Android SMS/MMS databases. The backup app does little conversion.

---

## SMS messages (`<sms>`)

| Attribute | Meaning |
|-----------|---------|
| `protocol` | Protocol id; usually `0` for SMS |
| `address` | Phone number of the other person |
| `date` | Sent/received time as Java ms UTC |
| `type` | `1` received, `2` sent, `3` draft, `4` outbox, `5` failed, `6` queued |
| `subject` | Subject; always null for SMS |
| `body` | Message text |
| `toa` | Unused; usually null |
| `sc_toa` | Unused; usually null |
| `service_center` | Service center for received messages; null for sent |
| `read` | `1` read, `0` unread |
| `status` | `-1` none, `0` complete, `32` pending, `64` failed |
| `sub_id` | Optional SIM / subscription index (`0`, `1`, …) |
| `readable_date` | Optional human-readable date string |
| `contact_name` | Optional contact display name |

---

## MMS messages (`<mms>`)

An MMS has three layers:

1. Attributes on `<mms>` (time, box, subject, address list)
2. Content in `<parts><part>…</part></parts>`
3. Recipients in `<addrs><addr>…</addr></addrs>`

### `<mms>` attributes

| Attribute | Meaning |
|-----------|---------|
| `date` | Sent/received time as Java ms UTC |
| `ct_t` | Message content type; usually `application/vnd.wap.multipart.related` |
| `msg_box` | `1` received, `2` sent, `3` draft, `4` outbox |
| `rr` | Read-report flag |
| `sub` | Subject, if any |
| `read_status` | Read-status flag |
| `address` | Phone number(s); group threads often use `~`-separated numbers |
| `m_id` | Message-ID from the MMS |
| `read` | Whether the message was read |
| `m_size` | Message size |
| `m_type` | MMS message type (MMS spec) |
| `sim_slot` | SIM card slot |
| `readable_date` | Optional human-readable date |
| `contact_name` | Optional contact display name |

### `<part>` attributes

| Attribute | Meaning |
|-----------|---------|
| `seq` | Order of the part |
| `ct` | Content type (`text/plain`, `image/jpeg`, `application/smil`, …) |
| `name` | Part name |
| `chset` | Charset |
| `cl` | Content location (often the filename used in SMIL) |
| `text` | Text content of the part |
| `data` | Base64-encoded binary content |

### `<addr>` attributes

| Attribute | Meaning |
|-----------|---------|
| `address` | Phone number of sender or recipient |
| `type` | `129` BCC, `130` CC, `151` To, `137` From |
| `charset` | Character set for this entry |

---

## Call logs (`<call>`) — not exported

| Attribute | Meaning |
|-----------|---------|
| `number` | Phone number of the call |
| `duration` | Duration in seconds |
| `date` | Time as Java ms UTC |
| `type` | `1` incoming, `2` outgoing, `3` missed, `4` voicemail, `5` rejected, `6` refused list |
| `presentation` | Caller ID: `1` allowed, `2` restricted, `3` unknown, `4` payphone |
| `subscription_id` | Optional SIM / subscription id |
| `readable_date` | Optional human-readable date |
| `contact_name` | Optional contact name |

Call rows can appear in the same backup XML. They are listed here so the file format is complete. The exporter ignores them (no call messages in IR / any projector).
