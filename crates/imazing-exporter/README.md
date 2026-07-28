# iMazing → common message → packaging

Convert [iMazing](https://imazing.com/) **Messages** and **WhatsApp** CSV exports into the [common message](../../docs/src/common-message.md), then package as JSON (default), CSV, EML, MBOX, JSONL, or XML. Enriches chat ids and display names from an iMazing **Contacts** CSV.

**Targeted upstream:** iMazing **3.5.5** (`export_tool` / `export_tool_version` on every output row).

Design notes and known limitations: [`docs/DESIGN.md`](docs/DESIGN.md).  
Library (`ExporterConfig` / `run`) for the GUI; thin CLI for standalone use. CLI reference: [`docs/MANPAGE.md`](docs/MANPAGE.md).

## What this is for

iMazing can export Messages and WhatsApp as CSV from an iPhone backup. Those files already have useful columns (`Chat Session`, `Message Date`, `Sender ID`, `Text`, `Attachment`, …), but:

- `Chat Session` is often a **name**, not a phone
- Outgoing rows leave `Sender ID` / `Sender Name` empty
- WhatsApp uses a different column set (no `Service`; has `Forwarded` / `Attachment info`)
- Attachment cells are filenames only unless you exported media with an All-backup export

This converter joins the CSVs with the Contacts CSV from the same backup so `chat_identifier` becomes E.164 when possible, and names fill in from the address book (including phones that iMazing stuffed into `Notes` as `PROP-ID: +…`).

WhatsApp conversations are written separately from SMS/iMessage (`…__whatsapp.csv`).

Unresolved name-only chats still write (name-based filename). That is not fatal here, but vault ingest may struggle until the contact book is complete.

This is **not** the same as [`imessage-ir-exporter`](../imessage-ir-exporter), which reads Apple’s `chat.db` on a Mac.

Shared CSV columns: [`docs/src/csv-output.md`](../../docs/src/csv-output.md). Design notes: [`docs/DESIGN.md`](docs/DESIGN.md).

## What you need

1. An iMazing export: one CSV, a chat folder, `Messages/`, `WhatsApp/`, or a full device export root
2. Contacts CSV from the same backup export (recommended via `--contacts`; without it a warning is printed and phones are not resolved to names)

Individual message selections may omit attachment data — export the whole conversation or All Messages when you care about media.

## How to run

From the [message-exporters](../..) repository root:

```bash
cargo run --release -p imazing-exporter -- \
  --input "/path/to/Device Export Root" \
  --output ./staging/imazing \
  --contacts "/path/to/Contacts - 2026-07-19.csv" \
  --timezone UTC-05:00
```

`--input` may be one Messages/WhatsApp CSV or any folder under the export (recursive). Contacts CSVs and `*attachment*` filenames are skipped.

`Message Date` has no timezone — pass `--timezone` as a fixed UTC offset (e.g. `UTC-05:00`) if the phone lived elsewhere; host local zone otherwise. Offsets do not observe DST.

Full CLI (dates, media modes, obfuscate): [`docs/MANPAGE.md`](docs/MANPAGE.md).

## Important limitations

- A Messages group member who never sends has **no phone** unless their name in `Chat Session` resolves through Contacts.
- WhatsApp CSVs have **no participant roster**; non-senders are invisible.
- Outgoing sender identity is blank in the vendor export.

See [`docs/DESIGN.md`](docs/DESIGN.md) for the full list.

## License

MIT.
