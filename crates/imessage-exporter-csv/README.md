# imessage-exporter-csv

Workspace fork of upstream [`imessage-exporter`](https://github.com/ReagentX/imessage-exporter) (`imessage-exporter` package only) that adds a **`csv`** export format.

- Binary: `imessage-exporter-csv`
- Default format: `csv`
- Baseline: upstream `txt` + `html` (no JSON exporter in this crate)
- SQLite parsers: crates.io [`imessage-database`](https://crates.io/crates/imessage-database)

The JSON-capable fork remains at [`crates/imessage-exporter`](../imessage-exporter) and is unrelated to this crate.

## Build

```bash
cargo build --release -p imessage-exporter-csv
```

## CSV export

```bash
imessage-exporter-csv -f csv -c clone -o csv_export
```

One `.csv` file per conversation. Columns follow the HTML message surface; values are filled from `chat.db` (handles, participants, RFC 3339 times). Nested structures use JSON cells (`parts_json`, `tapbacks_json`, `edits_json`, `attachments_json`, `app_json`).

Example output shape: [`samples/15551212.csv`](samples/15551212.csv). Each row includes `export_source=imessage` for [`message-vault-rs` csv-ingest](https://github.com/bitrealm-dev/message-vault-rs) mapping selection.

## Upstream sync

1. Copy a fresh `imessage-exporter/` package from upstream into this directory
2. Restore the CSV overlay: `src/exporters/csv/`, `ExportType::Csv`, binary/package rename, crates.io `imessage-database`, default `-f csv`
3. Smoke: `cargo build -p imessage-exporter-csv && imessage-exporter-csv -f csv -o /tmp/out`
