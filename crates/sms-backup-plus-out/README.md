# SMS Backup+ → CSV

Convert [SMS Backup+](https://github.com/jberkel/sms-backup-plus) email exports into one spreadsheet file per conversation, plus any photos or other media stored in those emails.

**Targeted upstream:** SMS Backup+ **1.5.11** (`export_tool` / `export_tool_version` on every output row).

## What this is for

SMS Backup+ can copy Android SMS and MMS into email (for example Gmail). People often download or archive those emails as `.eml` files — ordinary email files on disk. This converter reads a folder of those files. It does **not** sign in to email or talk to IMAP.

Backups usually show up in two shapes:

- **One file per message** — a short email that holds a single text or MMS
- **Archive emails** — a longer email that holds many messages from the same conversation in the body

How those emails are structured: [docs/FORMAT.md](docs/FORMAT.md). How each message becomes a spreadsheet row: [docs/EML_CSV_MAPPING.md](docs/EML_CSV_MAPPING.md).

## What you get

- One CSV file per conversation (a CSV is a plain table you can open in Excel, Numbers, or Google Sheets)
- An `attachments/` folder for media taken from the emails
- Each row is one message: who it was with, when it was sent or received, the text, and whether media was attached

If the same message appears more than once (for example in both a one-message file and an archive), the converter keeps a single copy. Details are in the [mapping doc](docs/EML_CSV_MAPPING.md).

Example output from test emails: [`sample-output/`](sample-output/).

## What you need

1. A folder of `.eml` files from SMS Backup+
2. **Your phone number** — so the converter can tell which messages you sent
3. **Your email address** — the address SMS Backup+ used when it mailed the texts (needed when the email does not clearly mark sent vs received)
4. **Contacts** (recommended) — `--contacts` (vault-shaped CSV) or `--vcf` for name↔phone resolution; without either, a warning is printed and names are left unresolved

Pass owner phone/email on the command line, or put them in `config/owner.toml` next to this converter (`phones = […]` and `emails = […]`; see `config/owner.example.toml`). Optional `--name-mapping` still defaults to `config/name-mapping.csv` when that file exists (crate-relative). Repeat `--owner-phone` or `--owner-email` if you used more than one number or address.

Also useful:

- `--name-mapping` — maps nicknames in the backup to the names you want in the export
- `-v` / `--verbose` — progress messages while large folders are scanned
- `--start-date` / `--end-date` — `YYYY-MM-DD` window `[start, end)` at host local midnight (end exclusive)

Contacts resolve **name → phone** when the EML has a name but no peer number, and **phone → name** when the display name is blank. Vault csv-ingest does not look up contacts.

## How to run

From the [message-exporters](../..) repository root:

```bash
cargo run --release -p sms-backup-plus-out -- convert \
  --input /path/to/eml_export \
  --output ./staging/sms-backup-plus \
  --owner-phone +15555550100 \
  --owner-email you@example.com \
  --contacts /path/to/contacts.csv
```

Replace the paths, phone number, and email with your own. `--input` is the folder of `.eml` files. `--output` is where the CSV files and `attachments/` folder are written. Use `--vcf` instead of `--contacts` if you have a VCF.

Add `--anonymize` (optional `--anonymize-seed <64-hex>`) to rewrite names, numbers, text, and attachments for sharing structure without PII. See [`message-anonymize`](../message-anonymize).

Attachment media: `--media-mode disabled|clone|convert|compress` (default `clone`). Convert/compress need `ffmpeg`/`ffprobe`. Compress options: `--media-max-resolution`, `--media-max-fps`, `--media-min-size`, `--media-skip-efficient`. See [`message-media`](../message-media).

## License

MIT.
