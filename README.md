# message-exporters

Backing up texts is easy. Getting the messages *out* in a form you can read is not.

Every backup app invents its own format—Android XML dumps, email archives, opaque PDU blobs, Apple’s Messages database. Those formats are built for restoring onto another phone, not for browsing history, searching, or keeping a durable archive. The tools rarely agree on timestamps, chat identity, or how media is stored, so the same conversation can look completely different depending on which app wrote the backup.

This repo exists to bridge that gap: turn those vendor-specific backups into plain CSV—one spreadsheet file per conversation, with photos and other media saved beside those files.

A CSV file is a plain table you can open in Excel, Numbers, Google Sheets, or any text editor. Each converter reads one kind of backup and writes those tables. Pick the converter that matches the app or device that created your backup.

### Why CSV?

Other formats look tempting. JSON is great for programs, but hard for a person to skim and check. HTML is great for presenting messages in a browser, but awkward as a structured store you can sort, filter, or re-import later.

CSV sits in the middle: human-readable enough that you can open a file and verify that the message text, times, and directions look right, yet still structured enough for spreadsheets and downstream tools. That makes it a practical archive format—and an easy place to catch export mistakes before you trust the data.

## Which converter to use

| Backup you have | Converter | Format docs |
|-----------------|-----------|-------------|
| **GO SMS Pro** local backup folder (Android) | [`go-sms-pro-to-csv`](crates/go-sms-pro-to-csv) | [How messages become spreadsheet rows](crates/go-sms-pro-to-csv/docs/XML_CSV_MAPPING.md) |
| **SMS Backup & Restore** XML from SyncTech (Android) | [`sms-backup-restore-to-csv`](crates/sms-backup-restore-to-csv) | [What the XML contains](crates/sms-backup-restore-to-csv/docs/FIELDS.md), [How messages become spreadsheet rows](crates/sms-backup-restore-to-csv/docs/XML_CSV_MAPPING.md) |
| **SMS Backup+** email exports (`.eml` files) | [`sms-backup-plus-to-csv`](crates/sms-backup-plus-to-csv) | [How the email backup is structured](crates/sms-backup-plus-to-csv/docs/FORMAT.md), [How messages become spreadsheet rows](crates/sms-backup-plus-to-csv/docs/EML_CSV_MAPPING.md) |
| **Apple Messages** database on a Mac (`chat.db`) | [`imessage-exporter`](crates/imessage-exporter) | [Converter README](crates/imessage-exporter/README.md), [example spreadsheet](crates/imessage-exporter/sample-output/15551212.csv) |

Each converter’s README explains what the backup looks like, what you need to run it, and the exact command.

## Build

These tools are written in Rust. From the repository root, build every converter with:

```bash
cargo build --workspace --release
```

The finished programs appear under `target/release/`.

## License

Most converters are MIT — see [LICENSE](LICENSE). `imessage-exporter` is GPL-3.0-or-later (inherited from upstream imessage-exporter).
