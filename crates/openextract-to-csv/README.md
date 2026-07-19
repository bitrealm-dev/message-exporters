# OpenExtract → CSV

Convert OpenExtract conversation CSV exports into one vault-shaped spreadsheet file per conversation, enriching phone numbers and names from the contacts `.vcf` that ships with the export.

## What this is for

OpenExtract writes thin CSV files such as:

- Per chat: `conversation_123.csv` with columns `Date,Sender,Text,Is From Me,Has Attachments`
- Combined: `all_conversations.csv` with extra `Conversation` and `Direction` columns

`Sender` may be a phone number, a display name, or `me`. That is enough to read the thread, but vault import wants stable phone chat ids and display names. Pass the export’s `.vcf` so phones resolve to names (and names to phones when possible).

If a contact is only known by name and the VCF has no phone for them, the converter still writes a CSV (name-based filename). That is not treated as a fatal error here, but vault ingest may struggle until the contact book is complete.

Attachment sidecar CSVs (`*_attachments.csv`) are ignored in this version; `attachments_json` is empty.

Example output: [`sample-output/`](sample-output/).

## What you need

1. OpenExtract conversation CSV file(s) — one file or a folder
2. The contacts `.vcf` from the same export (or a vault-shaped `--contacts` CSV)

## How to run

From the [message-exporters](../..) repository root:

```bash
cargo run --release -p openextract-to-csv -- \
  --input /path/to/openextract_csv_dir \
  --output ./staging/openextract \
  --vcf /path/to/contacts.vcf
```

`--input` may be a single `conversation_*.csv`, an `all_conversations.csv`, or a directory of those files. `*_attachments.csv` files are skipped automatically.

## License

MIT.
