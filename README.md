# message-exporters

Convert phone / app **message backups** into portable files (CSV, NDJSON, attachments).

This repo does **not** include the vault UI, SQLite import, or CSV→vault JSON ingest. Those live in [`message-vault-rs`](https://github.com/bitrealm-dev/message-vault-rs).

```text
message-exporters   backup / app export  →  CSV or SMS/iMessage NDJSON
message-vault-rs    CSV / vault NDJSON   →  SQLite + web UI
```

## Crates

| Crate | Input | Output |
|-------|--------|--------|
| `go-sms-pro-exporter-csv` | GO SMS Pro XML + PDU | per-chat CSV |
| `sms-backup-restore-exporter-csv` | SMS Backup & Restore XML | per-chat CSV |
| `sms-backup-plus-exporter-csv` | SMS Backup+ EML | per-chat CSV |
| `sms-backup-plus-exporter` | SMS Backup+ EML | SMS NDJSON (`message_json::sms`) |
| `imessage-exporter-csv` | iOS Messages DB | per-chat CSV |
| `imessage-exporter` | iOS Messages DB | JSON NDJSON (`imessage-exporter-json`) |
| `message-json` | — | shared NDJSON types for SMS/iMessage wire formats |

## Build

```bash
cargo build --workspace --release
```

Binaries land under `target/release/`. Point `message-vault-rs` ingest at them via `PATH` or `MESSAGE_EXPORTERS_BIN` (directory containing the release binaries).

## License

MIT — see [LICENSE](LICENSE).
