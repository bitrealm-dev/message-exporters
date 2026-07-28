# whatsapp-exporter

Convert WhatsApp Android/iOS databases (via [KnugiHK WhatsApp-Chat-Exporter](https://github.com/KnugiHK/WhatsApp-Chat-Exporter) `wtsexporter`) into this repo’s per-chat vault CSV layout.

This crate shells out to the `wtsexporter` CLI, then maps its JSON into CSV. It does **not** vendor or clone that project.

Library (`ExportConfig` / `run`) for the GUI; thin CLI for standalone use. Still shells out to `wtsexporter` for extract. CLI reference: [`docs/MANPAGE.md`](docs/MANPAGE.md).

## Install `wtsexporter`

**Developers (PyPI):**

```bash
pip install 'whatsapp-chat-exporter[android_backup,crypt15]'
# requires whatsapp-chat-exporter >= 0.13
```

**Release / GUI:** our GitHub Release workflow downloads KnugiHK’s prebuilt `wtsexporter` binary and places it next to `whatsapp-exporter`. The GUI also looks beside the exe, in `MESSAGE_EXPORTERS_BIN`, then on `PATH`. Override with `WTSEXPORTER=/path/to/wtsexporter`.

## Usage

Extraction runs in a temp directory under `--output` (not the process cwd). Full flags, iOS/`--json` examples, and environment variables: [`docs/MANPAGE.md`](docs/MANPAGE.md).

```bash
cargo run -p whatsapp-exporter -- \
  --platform android \
  --key /path/to/key-or-hex \
  --backup msgstore.db.crypt15 \
  --output ./staging/whatsapp
```

Output chats use the `__whatsapp` filename suffix (same convention as iMazing WhatsApp CSV).
