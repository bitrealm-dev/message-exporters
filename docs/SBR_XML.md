# SMS Backup & Restore XML output

Exporters can project canonical IR into a **single** SyncTech-style backup file:

`{output}/smses.xml`

Root shape:

```xml
<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<smses count="N">
  <sms … />
  <mms …>…</mms>
</smses>
```

This is the same family of files that [SMS Backup & Restore](https://www.synctech.com.au/sms-backup-restore/) reads. Attribute reference: [`crates/sms-backup-restore-exporter/docs/FIELDS.md`](../crates/sms-backup-restore-exporter/docs/FIELDS.md).

## Layout

| Piece | Crate / API |
|-------|-------------|
| Codec (write elements, finalize `count`) | [`message-sbr`](../crates/message-sbr/) |
| IR → SBR mapping + export sink | [`message_ir::FormatSink`](../crates/message-ir/) (XML path uses `SbrBackupSession` internally) |
| CLI / GUI | `--format xml` / `OutputFormat::Xml` |

Exporters use `FormatSink::open` → `write_document` per conversation → `finish`. Do **not** call `write_format(..., Xml, …)` (returns an error — a single shared file cannot be safely rewritten per chat).

## Mapping rules

- **SMS** when 1:1 and no attachments: `<sms>` with `type` `1`/`2`, `date` = `timestamp_unix_ms`, `body` = text.
- **MMS** when group and/or attachments (or `message_kind=mms`): `<mms>` with `<parts>` / `<addrs>`; attachment bytes base64 in `data` when available on disk or in memory.
- If `source.fields` has `kind: "sms"|"mms"` (as produced by the SBR importer), attrs / parts / addrs are preferred and overlaid with IR date/direction/body.
- **Dropped:** entire `imessage` bag (tapbacks, replies, balloons, send effects, edits, announcements, …). Text and media still export as SMS/MMS.

## Related

- [MESSAGE_IR.md](MESSAGE_IR.md) — IR projectors overview
- [XML_CSV_MAPPING.md](../crates/sms-backup-restore-exporter/docs/XML_CSV_MAPPING.md) — XML → CSV/IR (import direction)
