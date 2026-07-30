# Message Exporters Web GUI

Browser-based alternative to [`message-exporters-gui`](../message-exporters-gui) — a local
[axum](https://github.com/tokio-rs/axum) server with a server-rendered HTML UI, for platforms
(notably high-DPI Windows) where egui's rendering looks worse than a normal web page. Same
exporter libraries, same `export.ini`, all work runs in-process on your own machine; only the
presentation layer differs.

**End-user guides:** [docs site](https://bitrealm-dev.github.io/message-exporters/) (Install, desktop app, per-source export, Convert).

## Run in development

```bash
cargo build --workspace
cargo run -p message-exporters-web
```

This binds an OS-assigned port on `127.0.0.1`, prints the URL, and opens your default
browser to it. Stop the server with `Ctrl+C`.

For release:

```bash
cargo build --workspace --release
./target/release/message-exporters-web
```

On Windows the final command is `target\release\message-exporters-web.exe`.

The app searches for helper binaries (`wtsexporter`, `ffmpeg`, `ffprobe`) beside its own
executable, then in `MESSAGE_EXPORTERS_BIN`, then on `PATH`. Release ZIPs ship those helpers
next to the app.

## Included

- Top tabs: **Contacts** | **Export** | **Convert** | **Vault** | **Log**
- **Contacts**: Check (dry run) / Update (write corrected files)
- **Export**: backup-source picker, output formats (csv / eml / mbox / json / jsonl / xml), attachments, obfuscate, dates
- **Convert**: convert a prior Message Exporters output folder to another format
- **Vault**: push a JSONL export folder into Message Vault
- Forms for GO SMS Pro, SMS Backup & Restore, SMS Backup+, OpenExtract, iMazing, WhatsApp, and iPhone backup
- Server-side folder/file browse dialog (no native file picker in a browser)
- Live run log via Server-Sent Events, with cooperative cancel

Architecture and design notes: [`../../docs/maintainers/web-gui.md`](../../docs/maintainers/web-gui.md).
