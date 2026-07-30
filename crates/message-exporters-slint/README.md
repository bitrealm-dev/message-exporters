# Message Exporters Slint GUI

Desktop GUI built with [Slint](https://slint.dev) — an additive alternative to
[`message-exporters-gui`](../message-exporters-gui) (egui) and
[`message-exporters-web`](../message-exporters-web) (local browser UI). Same
exporter libraries, same `export.ini`; only the presentation layer differs.

**End-user guides:** [docs site](https://bitrealm-dev.github.io/message-exporters/).

## Run in development

```bash
cargo build --workspace
cargo run -p message-exporters-slint
```

For release:

```bash
cargo build --workspace --release
./target/release/message-exporters-slint
```

On Windows the final command is `target\release\message-exporters-slint.exe`.

The app searches for helper binaries (`wtsexporter`, `ffmpeg`, `ffprobe`) beside
its own executable, then in `MESSAGE_EXPORTERS_BIN`, then on `PATH`.

## Look and feel

Built with Slint's **`native`** widget style (set in `build.rs`):

- **Windows** — Fluent
- **macOS** — Cupertino
- **Linux** — Qt if Qt 5.15+ is installed; otherwise Fluent (pure-Rust fallback; no Qt SDK required)

Forms use a classic dialog grid (right-aligned label column, full-width
controls, ~4px row gaps, content packed at the top). Form rows use bare
`HorizontalLayout`/`VerticalLayout` — not `HorizontalBox`/`VerticalBox`, which
inject Fluent's 8px `layout-padding` per side and inflate every row. Ordinary
fields do not stretch when you grow the window vertically; only the Log viewer
does. Override the style at compile time with `SLINT_STYLE` if needed:

```bash
SLINT_STYLE=fluent cargo run -p message-exporters-slint
```

## Included

- Top tabs: **Contacts** | **Export** | **Convert** | **Vault** | **Log**
- **Contacts**: Check (dry run) / Update (write corrected files)
- **Export**: backup-source picker, output formats, attachments, obfuscate, dates
- **Convert**: convert a prior Message Exporters output folder to another format
- **Vault**: push a JSONL export folder into Message Vault
- Forms for GO SMS Pro, SMS Backup & Restore, SMS Backup+, OpenExtract, iMazing, WhatsApp, and iPhone backup
- Native file/folder dialogs via `rfd`
- Live run log with cooperative cancel
- About dialog with Slint attribution (`AboutSlint`) for the Royalty-free license

Architecture notes: [`../../docs/maintainers/slint-gui.md`](../../docs/maintainers/slint-gui.md).
