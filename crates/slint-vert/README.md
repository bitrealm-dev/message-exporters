# Message Exporters Vertical Slint GUI

Experimental, workspace-only variant of
[`message-exporters-slint`](../message-exporters-slint). It uses the same
exporter libraries and `export.ini`, but presents each tab as a top-to-bottom
workflow with labels above full-width controls.

**End-user guides:** [docs site](https://bitrealm-dev.github.io/message-exporters/).

## Run in development

```bash
cargo build --workspace
cargo run -p slint-vert
```

For release:

```bash
cargo build --workspace --release
./target/release/slint-vert
```

On Windows the final command is `target\release\slint-vert.exe`.

The app searches for helper binaries (`wtsexporter`, `ffmpeg`, `ffprobe`) beside
its own executable, then in `MESSAGE_EXPORTERS_BIN`, then on `PATH`.

## Look and feel

Built with Slint's **`native`** widget style (set in `build.rs`).

Forms follow a vertical workflow: each label sits above its full-width control,
path browse buttons remain beside their fields, and field groups are separated
by consistent vertical spacing. Scrollable tabs stay anchored at the top while
the Log viewer uses remaining height. Override the style at compile time with
`SLINT_STYLE` if needed:

```bash
SLINT_STYLE=fluent cargo run -p slint-vert
```

## Included

- Top tabs: **Extract Messages** | **Format** | **Vault** | **Contacts** | **Log**
- **Extract Messages**: choose a backup source and extract a JSONL archive; attachments, obfuscation, and optional date filters are available
- **Format**: convert a prior Message Exporters output folder to another format
- **Vault**: push a JSONL export folder into Message Vault
- **Contacts**: Check (dry run) / Update (write corrected files)
- Forms for GO SMS Pro, SMS Backup & Restore, SMS Backup+, OpenExtract, iMazing, WhatsApp, and iPhone backup
- Native file/folder dialogs via `rfd`
- Live run log with cooperative cancel
- About dialog with Slint attribution (`AboutSlint`) for the Royalty-free license

Shared architecture notes: [`../../docs/maintainers/slint-gui.md`](../../docs/maintainers/slint-gui.md).
