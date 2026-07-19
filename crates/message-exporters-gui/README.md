# Message Exporters GUI

Cross-platform [Iced](https://iced.rs/) desktop interface for the exporter binaries in this
workspace.

## Run in development

Build the exporters and GUI in the same profile so the GUI can find sibling executables:

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

The GUI searches for exporter binaries beside its own executable, then in
`MESSAGE_EXPORTERS_BIN`, then on `PATH`.

## Included

- Backup-source picker (alphabetical) with product/tool site links
- Global options panel: Anonymize (+ seed) and Start/End date for every source
- Attachments: Clone / Convert / Compress on sources that emit media (Compress shows resolution/fps/min-size options; needs ffmpeg)
- Forms for GO SMS Pro, SMS Backup & Restore, SMS Backup+, OpenExtract, iMazing, and iPhone backup
- Native file/folder dialogs
- OS-appropriate default output folders under Documents/`message-exporters`
- Exporter-specific validation and CLI argument generation
- Always-on run log (SMS Backup+ always runs with `--verbose`)
- Cancel for a running exporter
- Chevron-toggled advanced options and a flat light theme

See [`../../docs/GUI.md`](../../docs/GUI.md) for the full option matrix and architecture notes.
