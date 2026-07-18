# go-sms-pro-exporter-csv

Convert a **GO SMS Pro** (GOMO / Jiubang) local backup into per-conversation **CSV**. Reuses iMessage field names where the concept exists; unused iMessage-only columns are omitted. SMS-Pro-only fields are appended. A universal CSV shared by all exporters is a non-goal.

Part of the [message-vault-rs](../..) Cargo workspace. Field mapping: [`docs/XML_CSV_MAPPING.md`](docs/XML_CSV_MAPPING.md). Example output from the smoke fixture: [`samples/`](samples/).

## Input

A directory containing:

- `gosms_sys*.xml` — SMS (`<GoSms><SMS>…`)
- `I_<unix>_*.pdu` — MMS PDU blobs (attachments extracted by magic-byte heuristics)

There is no public GO SMS Pro backup spec; XML fields mirror [Android Telephony SMS columns](https://developer.android.com/reference/android/provider/Telephony.TextBasedSmsColumns). PDU parsing follows heuristics from the personal `message-vault` GoSMS converter (reference only).

## Standalone usage

From the repo root:

```bash
cargo run --release -p go-sms-pro-exporter-csv -- \
  --input /path/to/gosms_export \
  --output ./staging/go-sms-pro \
  --owner-phone +15555550100
```

Output:

- one `.csv` file per conversation (header row + one row per message)
- `attachments/` for media extracted from PDUs
- `service` is `"SMS"`
- SMS-Pro-only columns: `export_source` (`go-sms-pro`), `source_kind`, `android_type`, `date_ms`, `contact_name`, `pdu_filename`, `xml_fields_json`

## Vault ingest

`message-vault-rs ingest go-sms-pro` exports CSV into staging, then runs [`message-vault-rs` csv-ingest](https://github.com/bitrealm-dev/message-vault-rs) (mapping → imessage-shaped NDJSON) and continues with normal vault import.

## License

MIT (or match your vault repos).
