# message-exporters

Turn phone and messaging-app backups into one spreadsheet file per conversation, with photos and other media saved beside those files.

A CSV file is a plain table you can open in Excel, Numbers, Google Sheets, or any text editor. Each converter reads one kind of backup and writes those tables. Pick the converter that matches the app or device that created your backup.

## Which converter to use

| Backup you have | Converter | Format docs |
|-----------------|-----------|-------------|
| **GO SMS Pro** local backup folder (Android) | [`go-sms-pro-to-csv`](crates/go-sms-pro-to-csv) | [How messages become spreadsheet rows](crates/go-sms-pro-to-csv/docs/XML_CSV_MAPPING.md) |
| **SMS Backup & Restore** XML from SyncTech (Android) | [`sms-backup-restore-to-csv`](crates/sms-backup-restore-to-csv) | [What the XML contains](crates/sms-backup-restore-to-csv/docs/FIELDS.md), [How messages become spreadsheet rows](crates/sms-backup-restore-to-csv/docs/XML_CSV_MAPPING.md) |
| **SMS Backup+** email exports (`.eml` files) | [`sms-backup-plus-to-csv`](crates/sms-backup-plus-to-csv) | [How the email backup is structured](crates/sms-backup-plus-to-csv/docs/FORMAT.md), [How messages become spreadsheet rows](crates/sms-backup-plus-to-csv/docs/EML_CSV_MAPPING.md) |
| **Apple Messages** database on a Mac (`chat.db`) | [`imessage-exporter`](crates/imessage-exporter) | [Converter README](crates/imessage-exporter/README.md), [example spreadsheet](crates/imessage-exporter/samples/15551212.csv) |

Each converter’s README explains what the backup looks like, what you need to run it, and the exact command.

## Build

These tools are written in Rust. From the repository root, build every converter with:

```bash
cargo build --workspace --release
```

The finished programs appear under `target/release/`.

## License

Most converters are MIT — see [LICENSE](LICENSE). `imessage-exporter` is GPL-3.0-or-later (inherited from upstream imessage-exporter).
