# message-exporters

Phone backups are easy to make. Reading the messages later is harder.

This project turns vendor backups into plain files that a spreadsheet, mail program, or other tool can open. Pick the output format that fits the job—one size does not fit all:

- **CSV** — one spreadsheet file per conversation
- **EML** — one email folder per conversation
- **MBOX** — one mailbox file per conversation
- **JSON** / **JSON Lines** — machine-readable archives
- **XML** — one SyncTech `smses.xml` backup

Photos and other media are saved next to those files when the format needs them.

## Docs

Read the full guide (install, desktop app, supported backups, CSV layout):

**https://bitrealm-dev.github.io/message-exporters/**

Source Markdown lives in [`docs/`](docs/) (mdBook). Chapters also pull in crate READMEs.

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

Experimental converters also ship in the GUI and release zip: GO SMS Pro, iMazing CSV, OpenExtract, and SMS Backup+. Use those when they are the only backup on hand. Details: the [docs site](https://bitrealm-dev.github.io/message-exporters/) and [`docs/EXPORTER_MATRIX.md`](docs/EXPORTER_MATRIX.md).

## Releases

Prebuilt Linux, Windows, and macOS binaries: [Releases](https://github.com/bitrealm-dev/message-exporters/releases).

How maintainers cut a version: [`docs/DEVELOPING.md`](docs/DEVELOPING.md).

## License

Most converters are MIT — see [LICENSE](LICENSE). `imessage-ir-exporter` is GPL-3.0-or-later (via `imessage-database` / `crabapple`).
