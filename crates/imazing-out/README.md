# iMazing → CSV

Convert [iMazing](https://imazing.com/) **Messages** CSV exports into one vault-shaped spreadsheet file per conversation, enriching chat ids and display names from an iMazing **Contacts** CSV.

**Targeted upstream:** iMazing **3.5.5** (`export_tool` / `export_tool_version` on every output row).

## What this is for

iMazing can export Messages as CSV from an iPhone backup. Those files already have useful columns (`Chat Session`, `Message Date`, `Sender ID`, `Text`, `Attachment`, …), but:

- `Chat Session` is often a **name**, not a phone
- Outgoing rows leave `Sender ID` / `Sender Name` empty
- Attachment cells are filenames only unless you exported media with an All-backup Messages export

This converter joins the Messages CSV with the Contacts CSV from the same backup so `chat_identifier` becomes E.164 when possible, and names fill in from the address book (including phones that iMazing stuffed into `Notes` as `PROP-ID: +…`).

Unresolved name-only chats still write (name-based filename). That is not fatal here, but vault ingest may struggle until the contact book is complete.

This is **not** the same as [`imessage-exporter`](../imessage-exporter), which reads Apple’s `chat.db` on a Mac.

Example output: [`sample-output/`](sample-output/).

## What you need

1. Messages CSV from iMazing (prefer exporting from the **full / All backup** Messages view so attachments are listed)
2. Contacts CSV from the same backup export (recommended via `--contacts`; without it a warning is printed and phones are not resolved to names)

Individual message selections may omit attachment data — export the whole conversation or All Messages when you care about media.

## How to run

From the [message-exporters](../..) repository root:

```bash
cargo run --release -p imazing-out -- \
  --input "/path/to/Messages - Bob McRoy.csv" \
  --output ./staging/imazing \
  --contacts "/path/to/Contacts - 2026-07-19.csv" \
  --timezone America/New_York
```

`--input` may be one Messages CSV or a folder of them. Contacts CSVs and `*attachment*` filenames are skipped when scanning a directory.

`Message Date` values have no timezone. Pass `--timezone` (IANA name) if the phone lived in a different zone than this machine; otherwise the host local zone is used.

Attachments are recorded in `attachments_json` by filename only in this version (no media copy).

## License

MIT.
