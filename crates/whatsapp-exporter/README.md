# whatsapp-exporter

Convert WhatsApp Android/iOS databases (via [KnugiHK WhatsApp-Chat-Exporter](https://github.com/KnugiHK/WhatsApp-Chat-Exporter) `wtsexporter`) into this repo’s per-chat vault CSV layout.

This crate shells out to the `wtsexporter` CLI, then maps its JSON into CSV. It does **not** vendor or clone that project.

## Install `wtsexporter`

**Developers (PyPI):**

```bash
pip install 'whatsapp-chat-exporter[android_backup,crypt15]'
# requires whatsapp-chat-exporter >= 0.13
```

**Release / GUI:** our GitHub Release workflow downloads KnugiHK’s prebuilt `wtsexporter` binary and places it next to `whatsapp-exporter`. The GUI also looks beside the exe, in `MESSAGE_EXPORTERS_BIN`, then on `PATH`. Override with `WTSEXPORTER=/path/to/wtsexporter`.

## Usage

`--input` is optional (defaults to the process cwd) and is only used to resolve relative defaults such as `msgstore.db` / `wa.db` / `WhatsApp/`. Extraction always runs in a temporary directory under `--output` (cleaned up after convert), so the launch directory is not polluted. The GUI omits `--input`.

```bash
# Android crypt15 — run from the folder that contains the backup, or pass --input
cargo run -p whatsapp-exporter -- \
  --platform android \
  --key 133735053b5204b08e5c3823423399aa30ff061435ab89bc4e6713969cda1337 \
  --backup msgstore.db.crypt15 \
  --output ./staging/whatsapp

# iOS backup (--backup required)
cargo run -p whatsapp-exporter -- \
  --platform ios \
  --backup ~/Library/Application\ Support/MobileSync/Backup/DEVICE_ID \
  --output ./staging/whatsapp

# Convert-only (no Python) from an existing result.json
cargo run -p whatsapp-exporter -- \
  --json /path/to/result.json \
  --output ./staging/whatsapp
```

Optional forwards: `--wa`, `--media`, `--db`, `--business`, `--input`. Shared flags: `--start-date` / `--end-date`, `--media-mode`, `--obfuscate` / `--obfuscate-seed`.

Filenames use the shared `__whatsapp` suffix so they align with iMazing WhatsApp exports.
