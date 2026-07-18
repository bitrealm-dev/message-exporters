# Apple Messages → CSV

Export conversations from Apple Messages into one spreadsheet file per chat, and copy message attachments into a folder next to those files.

## What this is for

On a Mac, Messages stores history in a database file commonly called `chat.db`. This converter reads that database and writes CSV files — plain tables you can open in Excel, Numbers, or Google Sheets.

It can also write plain text or HTML if those formats are preferred. CSV is the default here.

## What you get

- One CSV file per conversation
- Attachment files copied into the output folder when copy mode is used
- Each row is one message: who sent it, when, the text, and related details such as reactions or edits when they exist in the database

Example spreadsheet shape: [`samples/15551212.csv`](samples/15551212.csv).

## What you need

1. Access to the Messages database on the Mac (the usual path is under the user Library folder; macOS may require Full Disk Access for the terminal)
2. A place to write the export

Unlike the Android converters in this repository, this tool does not ask for an “owner phone” flag. Direction comes from the Messages database itself.

## How to run

Build from the [message-exporters](../..) repository root:

```bash
cargo build --release -p imessage-exporter
```

Then export (default format is CSV; `-c clone` copies attachments into the output folder):

```bash
./target/release/imessage-exporter -f csv -c clone -o ./staging/imessage
```

Or run without installing the binary first:

```bash
cargo run --release -p imessage-exporter -- -f csv -c clone -o ./staging/imessage
```

## License

GPL-3.0-or-later (same as upstream [imessage-exporter](https://github.com/ReagentX/imessage-exporter)).

## Maintainer notes

This directory is a workspace copy of the upstream `imessage-exporter` package with a CSV export path added. SQLite reading uses the crates.io [`imessage-database`](https://crates.io/crates/imessage-database) crate. Upstream still provides `txt` and `html` export; this tree does not ship a JSON exporter.

To refresh from upstream:

1. Copy a fresh upstream `imessage-exporter/` package into this directory
2. Restore the CSV overlay (`src/exporters/csv/`, CSV as a selectable format, default `-f csv`, package/binary name `imessage-exporter`, crates.io `imessage-database`)
3. Smoke-test: `cargo build -p imessage-exporter && imessage-exporter -f csv -o /tmp/out`
