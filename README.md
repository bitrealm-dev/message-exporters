# message-exporters

Backing up texts is easy. Getting the messages *out* in a form you can read is not.

Every backup app invents its own format—Android XML dumps, email archives, opaque PDU blobs, Apple’s Messages database. Those formats are built for restoring onto another phone, not for browsing history, searching, or keeping a durable archive. The tools rarely agree on timestamps, chat identity, or how media is stored, so the same conversation can look completely different depending on which app wrote the backup.

This repo exists to bridge that gap: turn those vendor-specific backups into plain CSV—one spreadsheet file per conversation, with photos and other media saved beside those files.

A CSV file is a plain table you can open in Excel, Numbers, Google Sheets, or any text editor. Each converter reads one kind of backup and writes those tables. Pick the converter that matches the app or device that created your backup.

## Why CSV?

Other formats look tempting. JSON is great for programs, but hard for a person to skim and check. HTML is great for presenting messages in a browser, but awkward as a structured store you can sort, filter, or re-import later.

CSV sits in the middle: human-readable enough that you can open a file and verify that the message text, times, and directions look right, yet still structured enough for spreadsheets and downstream tools. That makes it a practical archive format—and an easy place to catch export mistakes before you trust the data.

## Contacts (Android exporters)

Android converters **require** a contacts file so names and phone numbers are resolved when the CSV is written (not later in the vault):

- `--contacts path/to/contacts.csv` — vault-shaped `phones,first_name,last_name[,exclude,…]` (phones `;`-separated), or
- `--vcf path/to/contacts.vcf` — same index from a VCF

Pass exactly one. Shared logic: [`crates/message-contacts`](crates/message-contacts). Open the CSV afterward and fix anything still wrong before vault import.

## Which converter to use

| Backup you have | Converter | Targeted upstream | Format docs |
|-----------------|-----------|-------------------|-------------|
| **GO SMS Pro** local backup folder (Android) | [`go-sms-pro-to-csv`](crates/go-sms-pro-to-csv) | GO SMS Pro *(version TBD)* | [How messages become spreadsheet rows](crates/go-sms-pro-to-csv/docs/XML_CSV_MAPPING.md) |
| **SMS Backup & Restore** XML from SyncTech (Android) | [`sms-backup-restore-to-csv`](crates/sms-backup-restore-to-csv) | SMS Backup & Restore **10.26.003** | [What the XML contains](crates/sms-backup-restore-to-csv/docs/FIELDS.md), [How messages become spreadsheet rows](crates/sms-backup-restore-to-csv/docs/XML_CSV_MAPPING.md) |
| **SMS Backup+** email exports (`.eml` files) | [`sms-backup-plus-to-csv`](crates/sms-backup-plus-to-csv) | SMS Backup+ **1.5.11** | [How the email backup is structured](crates/sms-backup-plus-to-csv/docs/FORMAT.md), [How messages become spreadsheet rows](crates/sms-backup-plus-to-csv/docs/EML_CSV_MAPPING.md) |
| **OpenExtract** conversation CSV + contacts `.vcf` | [`openextract-to-csv`](crates/openextract-to-csv) | OpenExtract **0.5.1** | [Converter README](crates/openextract-to-csv/README.md), [example spreadsheet](crates/openextract-to-csv/sample-output/_15555550122.csv) |
| **iMazing** Messages CSV + Contacts CSV | [`imazing-to-csv`](crates/imazing-to-csv) | iMazing **3.5.5** | [Converter README](crates/imazing-to-csv/README.md), [example spreadsheet](crates/imazing-to-csv/sample-output/_13212462167.csv) |
| **Apple Messages** database on a Mac (`chat.db`) | [`imessage-exporter`](crates/imessage-exporter) | iMessage Exporter **4.2.0** | [Converter README](crates/imessage-exporter/README.md), [example spreadsheet](crates/imessage-exporter/sample-output/15551212.csv) |

Each converter writes `export_source`, `export_tool`, and `export_tool_version` on every CSV row so downstream vault import knows which upstream tool/version the export targets.

Raw **iMazing** vendor Messages CSV can also be ingested by [message-vault-rs](https://github.com/bitrealm-dev/message-vault-rs) `csv-ingest` (no contact enrichment). Prefer [`imazing-to-csv`](crates/imazing-to-csv) when you have the Contacts export. To share structure without PII, rewrite vendor CSV with [`imazing-anonymize`](crates/message-anonymize).

Each converter’s README explains what the backup looks like, what you need to run it, and extra options.

## Anonymize (share structure, not PII)

Add `--anonymize` (optional `--anonymize-seed <64-hex>`) to any converter to rewrite names, numbers, message text (same length), and attachments after export. Remaps are stable for a given seed and not reversible from the CSV alone; the seed is printed to stderr when generated. Details: [`crates/message-anonymize`](crates/message-anonymize).

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
  --owner-phone +15555550100 \
  --contacts /path/to/contacts.csv
```

`--input` is the backup folder (XML + `.pdu` files). `--owner-phone` and `--contacts` (or `--vcf`) are required.

### SMS Backup & Restore (SyncTech XML)

```bash
cargo run --release -p sms-backup-restore-to-csv -- \
  --input /path/to/sms-20210328165031.xml \
  --output ./staging/sms-backup-restore \
  --owner-phone +15555550100 \
  --contacts /path/to/contacts.csv
```

`--input` may be one `.xml` file or a folder of them. Unlock/unzip encrypted `.zip` backups first. `--owner-phone` and `--contacts` (or `--vcf`) are required.

### SMS Backup+ (folder of `.eml` files)

```bash
cargo run --release -p sms-backup-plus-to-csv -- convert \
  --input /path/to/eml_export \
  --output ./staging/sms-backup-plus \
  --owner-phone +15555550100 \
  --owner-email you@example.com \
  --contacts /path/to/contacts.csv
```

Or put phone/email in [`crates/sms-backup-plus-to-csv/config/owner.toml`](crates/sms-backup-plus-to-csv/config/owner.example.toml) instead of the flags. Contacts still require `--contacts` or `--vcf` on the CLI.

### OpenExtract (conversation CSV + VCF)

```bash
cargo run --release -p openextract-to-csv -- \
  --input /path/to/openextract_csv_dir \
  --output ./staging/openextract \
  --vcf /path/to/contacts.vcf
```

`--input` is a conversation CSV or a folder of them (`all_conversations.csv` or `conversation_*.csv`). `--vcf` is the contacts file from the same export (phone ↔ name). Name-only chats still write; vault import may need a fuller VCF later.

### iMazing (Messages CSV + Contacts CSV)

```bash
cargo run --release -p imazing-to-csv -- \
  --input "/path/to/Messages - Someone.csv" \
  --output ./staging/imazing \
  --contacts "/path/to/Contacts - ….csv" \
  --timezone America/New_York
```

Export Messages from the **All backup** view when you want attachment filenames. `--contacts` is the iMazing Contacts CSV from the same backup. Distinct from `imessage-exporter` (which reads `chat.db`).

### Apple Messages (`chat.db` on a Mac)

```bash
cargo run --release -p imessage-exporter -- \
  -f csv -c clone -o ./staging/imessage
```

No owner phone flag. Use `-c clone` so attachments are copied into the output folder (otherwise the CSV keeps absolute paths into the Messages library). Full Disk Access may be required on macOS.

After a run, open the CSV files under `--output` / `-o` and check that times, direction, and text look right. Photos and other media are under `attachments/` (or the iMessage output folder when using `-c clone`).

### Anonymized share copy

```bash
# any converter — add after the usual flags:
  --anonymize

# iMazing vendor CSV (rewrite only):
cargo run --release -p message-anonymize --bin imazing-anonymize -- \
  --input /path/to/imazing.csv \
  --output ./staging/imazing-anon
```

## Releases

Prebuilt Linux, Windows, and macOS binaries are on the [Releases](https://github.com/bitrealm-dev/message-exporters/releases) page when a maintainer has cut one. How to publish a new version: [docs/DEVELOPING.md](docs/DEVELOPING.md).

## License

Most converters are MIT — see [LICENSE](LICENSE). `imessage-exporter` is GPL-3.0-or-later (inherited from upstream imessage-exporter).
