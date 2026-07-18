# GO SMS Pro → CSV

Convert a **GO SMS Pro** (GOMO / Jiubang) local backup into one spreadsheet file per conversation, plus any photos or other media found in the backup.

## What this is for

GO SMS Pro can save texts onto the phone as a backup folder. That folder usually contains:

- XML files named like `gosms_sys….xml` — ordinary SMS text messages
- files ending in `.pdu` — MMS messages (often with pictures or other media packed inside)

There is no official public description of this backup format. For a detailed walkthrough of how each message becomes a spreadsheet row, see [docs/XML_CSV_MAPPING.md](docs/XML_CSV_MAPPING.md).

## What you get

- One CSV file per conversation (a CSV is a plain table you can open in Excel, Numbers, or Google Sheets)
- An `attachments/` folder next to those files for media pulled out of MMS backups
- Each row is one message: who it was with, when it was sent or received, the text, and whether media was attached

Example output from a small test backup: [`samples/`](samples/).

## What you need

1. The GO SMS Pro backup folder on disk
2. **Your phone number** — the number that owned the messages on that phone (required; there is no demo default)

For ordinary SMS in the XML backup, sent vs received comes from the backup’s own type field. Your number is still required so MMS (`.pdu`) direction and chat grouping are correct. For example, if your number is `+1 555 555 0100`, pass that (or the same digits without spaces) as `--owner-phone`.

## How to run

From the [message-exporters](../..) repository root:

```bash
cargo run --release -p go-sms-pro-to-csv -- \
  --input /path/to/gosms_export \
  --output ./staging/go-sms-pro \
  --owner-phone +15555550100
```

Replace the paths and phone number with your own. `--input` is the backup folder. `--output` is where the CSV files and `attachments/` folder are written.

## License

MIT.
