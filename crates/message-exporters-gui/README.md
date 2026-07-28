# Message Exporters GUI

Cross-platform [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) desktop interface for the exporters in this workspace. Convert runs via linked exporter libraries; `contacts-validate` and WhatsApp `wtsexporter` still resolve as sibling tools when needed.

## Run in development

Build the workspace (GUI + optional sibling tools for contacts-validate / WhatsApp):

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

The GUI searches for helper binaries (`contacts-validate`, `wtsexporter`) beside its own executable, then in `MESSAGE_EXPORTERS_BIN`, then on `PATH`.

## Included

- Top tab panel: **Validate contacts** (default) | **Export**
- **Validate contacts**: Check (dry run) / Update (write corrected files) / Cancel
- Backup-source picker with product/tool site links
- Global options: Obfuscate (+ seed) and Start/End date for every source
- Attachments: Copy / Convert / Compress on sources that emit media (Compress shows resolution/fps/min-size options; needs ffmpeg)
- Forms for GO SMS Pro, SMS Backup & Restore, SMS Backup+, OpenExtract, iMazing, WhatsApp, and iPhone backup
- Native file/folder dialogs
- OS-appropriate default output folders under Documents/`message-exporters`
- Exporter-specific validation; in-process library export for every backup source
- Shared run log with cancel

See [`../../docs/GUI.md`](../../docs/GUI.md) for the full option matrix and architecture notes.
