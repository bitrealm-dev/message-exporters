# Message Exporters GUI

Cross-platform [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) desktop interface for the exporters in this workspace. Convert runs via linked exporter libraries; `contacts-validate` and WhatsApp `wtsexporter` still resolve as sibling tools when needed.

**End-user guides:** [docs site](https://bitrealm-dev.github.io/message-exporters/) (Install, desktop app, per-source export, Convert).

## Run in development

```bash
cargo build --workspace
cargo run -p message-exporters-gui
```

For release:

```bash
cargo build --workspace --release
./target/release/message-exporters-gui
```

On Windows the final command is `target\release\message-exporters-gui.exe`.

The GUI searches for helper binaries (`contacts-validate`, `wtsexporter`, `ffmpeg`, `ffprobe`) beside its own executable, then in `MESSAGE_EXPORTERS_BIN`, then on `PATH`. Release ZIPs ship those helpers next to the app.

## Included

- Top tabs: **Contacts** | **Export** | **Convert** | **Log**
- **Contacts**: Check (dry run) / Update (write corrected files) / Cancel
- **Export**: backup-source picker, output formats (csv / eml / mbox / json / jsonl / xml), attachments, obfuscate, dates
- **Convert**: convert a prior Message Exporters output folder to another format
- Forms for GO SMS Pro, SMS Backup & Restore, SMS Backup+, OpenExtract, iMazing, WhatsApp, and iPhone backup
- Native file/folder dialogs
- Shared run log with cancel

Contributor option matrix: [`../../docs/maintainers/gui.md`](../../docs/maintainers/gui.md).
