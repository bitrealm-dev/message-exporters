# message-exporters

Backing up texts is easy. Getting the messages *out* in a form you can read is not.

This repo turns vendor-specific phone backups into **plain CSV**—one spreadsheet file per conversation, with media beside those files.

## Docs

Full documentation (install, GUI, supported exporters, CSV shape):

**https://bitrealm-dev.github.io/message-exporters/**

(Source: [`docs/`](docs/) — mdBook; chapters pull in Markdown from `docs/` and crate READMEs.)

## Quick start

**GUI / binaries:** download the latest [Release](https://github.com/bitrealm-dev/message-exporters/releases), keep the tools in one folder, run `message-exporters-gui`.

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

Experimental (also in the GUI / release zip): GO SMS Pro, iMazing CSV, OpenExtract, SMS Backup+. See the [docs site](https://bitrealm-dev.github.io/message-exporters/) and [`docs/EXPORTER_MATRIX.md`](docs/EXPORTER_MATRIX.md).

## Releases

Prebuilt Linux, Windows, and macOS binaries: [Releases](https://github.com/bitrealm-dev/message-exporters/releases).  
How maintainers cut a version: [`docs/DEVELOPING.md`](docs/DEVELOPING.md).

## License

Most converters are MIT — see [LICENSE](LICENSE). `imessage-ir-exporter` is GPL-3.0-or-later (via `imessage-database` / `crabapple`).
