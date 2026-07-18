# message-exporters

Turn phone and app message backups into per-conversation CSV files (plus attachments).

Each crate reads one backup format and writes one CSV per chat. Column names overlap where the concept is shared (especially with iMessage), but there is no universal schema — source-specific fields stay on that source’s exporter.

## Why CSV

CSV is boring on purpose: open it in a spreadsheet, diff it, pipe it, or ingest it later without inventing another wire format. Nested bits (tapbacks, edits, raw XML crumbs) live in JSON cells when a flat column would lie.

## Crates

| Crate | Input | Format docs |
|-------|--------|-------------|
| [`go-sms-pro-to-csv`](crates/go-sms-pro-to-csv) | GO SMS Pro XML + PDU | [XML → CSV](crates/go-sms-pro-to-csv/docs/XML_CSV_MAPPING.md) |
| [`sms-backup-restore-to-csv`](crates/sms-backup-restore-to-csv) | SMS Backup & Restore XML | [fields](crates/sms-backup-restore-to-csv/docs/FIELDS.md), [XML → CSV](crates/sms-backup-restore-to-csv/docs/XML_CSV_MAPPING.md) |
| [`sms-backup-plus-to-csv`](crates/sms-backup-plus-to-csv) | SMS Backup+ EML | [EML format](crates/sms-backup-plus-to-csv/docs/FORMAT.md), [EML → CSV](crates/sms-backup-plus-to-csv/docs/EML_CSV_MAPPING.md) |
| [`imessage-to-csv`](crates/imessage-to-csv) | iOS Messages DB | [crate README](crates/imessage-to-csv/README.md), [sample CSV](crates/imessage-to-csv/samples/15551212.csv) |

Usage and flags live in each crate’s README.

## Build

```bash
cargo build --workspace --release
```

Binaries land under `target/release/`.

## License

MIT — see [LICENSE](LICENSE). `imessage-to-csv` is GPL-3.0-or-later (upstream imessage-exporter).
