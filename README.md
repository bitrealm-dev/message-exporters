# message-exporters

Backing up texts is easy. Getting the messages *out* in a form you can read is not.

Every backup app invents its own format—Android XML dumps, email archives, opaque PDU blobs, Apple’s Messages database. Those formats are built for restoring onto another phone, not for browsing history, searching, or keeping a durable archive. The tools rarely agree on timestamps, chat identity, or how media is stored, so the same conversation can look completely different depending on which app wrote the backup.

This repo exists to bridge that gap: turn those vendor-specific backups into plain CSV—one spreadsheet file per conversation, with photos and other media saved beside those files.

A CSV file is a plain table you can open in Excel, Numbers, Google Sheets, or any text editor. Each converter reads one kind of backup and writes those tables. Pick the converter that matches the app or device that created your backup.

## Why CSV?

Other formats look tempting. JSON is great for programs, but hard for a person to skim and check. HTML is great for presenting messages in a browser, but awkward as a structured store you can sort, filter, or re-import later.

CSV sits in the middle: human-readable enough that you can open a file and verify that the message text, times, and directions look right, yet still structured enough for spreadsheets and downstream tools. That makes it a practical archive format—and an easy place to catch export mistakes before you trust the data.

## Which converter to use

| Backup you have | Converter | Format docs |
|-----------------|-----------|-------------|
| **GO SMS Pro** local backup folder (Android) | [`go-sms-pro-to-csv`](crates/go-sms-pro-to-csv) | [How messages become spreadsheet rows](crates/go-sms-pro-to-csv/docs/XML_CSV_MAPPING.md) |
| **SMS Backup & Restore** XML from SyncTech (Android) | [`sms-backup-restore-to-csv`](crates/sms-backup-restore-to-csv) | [What the XML contains](crates/sms-backup-restore-to-csv/docs/FIELDS.md), [How messages become spreadsheet rows](crates/sms-backup-restore-to-csv/docs/XML_CSV_MAPPING.md) |
| **SMS Backup+** email exports (`.eml` files) | [`sms-backup-plus-to-csv`](crates/sms-backup-plus-to-csv) | [How the email backup is structured](crates/sms-backup-plus-to-csv/docs/FORMAT.md), [How messages become spreadsheet rows](crates/sms-backup-plus-to-csv/docs/EML_CSV_MAPPING.md) |
| **Apple Messages** database on a Mac (`chat.db`) | [`imessage-exporter`](crates/imessage-exporter) | [Converter README](crates/imessage-exporter/README.md), [example spreadsheet](crates/imessage-exporter/sample-output/15551212.csv) |

Each converter’s README explains what the backup looks like, what you need to run it, and extra options.

## Quick start

You need [Rust](https://www.rust-lang.org/tools/install) (`cargo`) on the machine that will run the converter. Clone this repository, then from the repo root build once:

```bash
cargo build --workspace --release
```

Binaries land under `target/release/`. Replace the example paths and identity values with your own, then run the matching command.

### GO SMS Pro (Android backup folder)

```bash
cargo run --release -p go-sms-pro-to-csv -- \
  --input /path/to/gosms_export \
  --output ./staging/go-sms-pro \
  --owner-phone +15555550100
```

`--input` is the backup folder (XML + `.pdu` files). `--owner-phone` is required (the number that owned the phone).

### SMS Backup & Restore (SyncTech XML)

```bash
cargo run --release -p sms-backup-restore-to-csv -- \
  --input /path/to/sms-20210328165031.xml \
  --output ./staging/sms-backup-restore \
  --owner-phone +15555550100
```

`--input` may be one `.xml` file or a folder of them. Unlock/unzip encrypted `.zip` backups first. `--owner-phone` is required.

### SMS Backup+ (folder of `.eml` files)

```bash
cargo run --release -p sms-backup-plus-to-csv -- convert \
  --input /path/to/eml_export \
  --output ./staging/sms-backup-plus \
  --owner-phone +15555550100 \
  --owner-email you@example.com
```

Or put phone/email in [`crates/sms-backup-plus-to-csv/config/owner.toml`](crates/sms-backup-plus-to-csv/config/owner.example.toml) instead of the flags.

### Apple Messages (`chat.db` on a Mac)

```bash
cargo run --release -p imessage-exporter -- \
  -f csv -c clone -o ./staging/imessage
```

No owner phone flag. Use `-c clone` so attachments are copied into the output folder (otherwise the CSV keeps absolute paths into the Messages library). Full Disk Access may be required on macOS.

After a run, open the CSV files under `--output` / `-o` and check that times, direction, and text look right. Photos and other media are under `attachments/` (or the iMessage output folder when using `-c clone`).

## License

Most converters are MIT — see [LICENSE](LICENSE). `imessage-exporter` is GPL-3.0-or-later (inherited from upstream imessage-exporter).
