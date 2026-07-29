# message-exporters

Phone backups are easy to make. Reading the messages later is harder.

This project turns vendor backups into a shared [conversation structure](docs/src/content/docs/understand-output/export-structure.md), then packages each conversation in the format you pick (default **JSON**):

- **JSON** / **JSON Lines** — default packaging; machine-readable archives
- **CSV** — one spreadsheet file per conversation
- **EML** — one email folder per conversation
- **MBOX** — one mailbox file per conversation
- **XML** — one SyncTech `smses.xml` backup

Photos and other media are saved next to those files when the format needs them.

## Docs

Read the full guide (install, desktop app, supported backups, CSV layout):

**https://bitrealm-dev.github.io/message-exporters/**

Source Markdown lives in [`docs/src/content/docs/`](docs/src/content/docs/) and is published with Astro Starlight.

## Quick start

**Desktop app / binaries:** Download the latest [Release](https://github.com/bitrealm-dev/message-exporters/releases). Keep every tool from the zip in the same folder. Run `message-exporters-gui`.

**From source:**

```bash
cargo build --workspace --release
cargo run --release -p message-exporters-gui
```

## Supported exporters

| Backup | Converter |
|--------|-----------|
| Apple Messages (`chat.db`) | [`imessage-ir-exporter`](crates/imessage-ir-exporter) |
| SMS Backup & Restore (SyncTech XML) | [`sms-backup-restore-exporter`](crates/sms-backup-restore-exporter) |
| WhatsApp (native DB / crypt) | [`whatsapp-exporter`](crates/whatsapp-exporter) |

Experimental converters also ship in the GUI and release zip: GO SMS Pro, iMazing CSV, OpenExtract, and SMS Backup+. Use those when they are the only backup on hand. Details: the [docs site](https://bitrealm-dev.github.io/message-exporters/) and [exporter capability matrix](docs/maintainers/exporter-matrix.md).

Already exported? The GUI **Re-export** tab ([`message-reexport`](crates/message-ir/docs/REEXPORT.md)) converts a prior output folder to another format (CSV ↔ EML ↔ MBOX ↔ JSON ↔ JSONL ↔ XML).

Import into Message Vault with the GUI **Vault** tab or the [`vault-push`](crates/vault-push/docs/MANPAGE.md) CLI (JSONL export folder + Import API token).

## Releases

Prebuilt Linux, Windows, and macOS binaries: [Releases](https://github.com/bitrealm-dev/message-exporters/releases).

Maintainer documentation: [`docs/maintainers/`](docs/maintainers/README.md). Release steps: [Development and releases](docs/maintainers/developing.md).

## License

Most converters are MIT — see [LICENSE](LICENSE). `imessage-ir-exporter` is GPL-3.0-or-later (via `imessage-database` / `crabapple`).
