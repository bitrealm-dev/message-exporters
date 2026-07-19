# SMS Backup & Restore → CSV

Convert an Android **SMS Backup & Restore** backup into one spreadsheet file per conversation, plus decoded MMS photos and other media.

**Targeted upstream:** SMS Backup & Restore **10.26.003** (`export_tool` / `export_tool_version` on every output row).

## What this is for

[SMS Backup & Restore](https://www.synctech.com.au/sms-backup-restore/) (by SyncTech) writes a backup file whose name looks like `sms-20210328165031.xml`. That file holds SMS and MMS from the phone’s messaging database.

This converter reads that XML and writes one CSV file per conversation. A CSV is a plain table you can open in Excel, Numbers, or Google Sheets.

- What the backup XML contains: [docs/FIELDS.md](docs/FIELDS.md)
- How each message becomes a spreadsheet row: [docs/XML_CSV_MAPPING.md](docs/XML_CSV_MAPPING.md)

## What you get

- One CSV file per conversation
- An `attachments/` folder for pictures and other media that were inside MMS messages
- Each row is one message: who it was with, when it was sent or received, the text, and whether media was attached

Example output from a small test backup: [`sample-output/`](sample-output/).

## What you need

1. Either one `sms-….xml` file, or a folder that contains several `.xml` backups (all of them are combined into one export)
2. **Your phone number** — the number that owned the messages on that phone (required; there is no demo default)
3. **Contacts** (recommended) — `--contacts` (vault-shaped CSV) or `--vcf` so blank display names can be filled from phone numbers; without either, a warning is printed and names are left unresolved

For ordinary SMS, sent vs received comes from the backup’s own type field. Your number is still required so MMS chat keys, group membership, and senders are correct. For example, if your number is `+1 555 555 0100`, pass that (or the same digits without spaces) as `--owner-phone`.

If the backup app gave you an encrypted `.zip` file, unlock and unzip it first. Point `--input` at the XML file inside the unzipped folder. This converter does not open encrypted archives.

## How to run

From the [message-exporters](../..) repository root:

```bash
cargo run --release -p sms-backup-restore-out -- \
  --input /path/to/sms-20210328165031.xml \
  --output ./staging/sms-backup-restore \
  --owner-phone +15555550100 \
  --contacts /path/to/contacts.csv
```

Add `--anonymize` (optional `--anonymize-seed <64-hex>`) to rewrite names, numbers, text, and attachments for sharing structure without PII. See [`message-anonymize`](../message-anonymize).

Replace the paths and phone number with your own. `--input` may be a single XML file or a directory of XML files. `--output` is where the CSV files and `attachments/` folder are written. Use `--vcf` instead of `--contacts` if you have a VCF.

## License

MIT.
