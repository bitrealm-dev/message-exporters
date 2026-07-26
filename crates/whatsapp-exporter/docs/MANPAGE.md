# NAME

whatsapp-exporter - convert WhatsApp DB/backup (via wtsexporter) to per-conversation CSV

# SYNOPSIS

```text
whatsapp-exporter --output <DIR> --platform android|ios
    [--input <PATH>] [--json <PATH>]
    [--key <KEY|PATH>] [--backup <PATH>] [--wa <PATH>] [--media <PATH>] [--db <PATH>]
    [--business]
    [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
    [--media-mode disabled|clone|convert|compress]
    [--media-max-resolution 720p|1080p|4k] [--media-max-fps <N>]
    [--media-min-size <SIZE>] [--media-skip-efficient true|false]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

# DESCRIPTION

Shells out to KnugiHK **wtsexporter**, then maps its JSON into vault-shaped per-conversation CSV (`*__whatsapp.csv`). Extraction runs in a temporary directory under `--output` (removed after convert) so the process launch directory is not polluted.

`--platform` is required unless `--json` is used (convert-only, no Python). The GUI does not pass `--input`.

# OPTIONS

**--output** *DIR*
: Destination for CSV, `attachments/`, and `wtsexporter_result.json`.

**--platform** *android|ios*
: Target platform for wtsexporter (`-a` / `-i`). Required unless `--json`.

**--input** *PATH*
: Search root for relative defaults (`msgstore.db`, `wa.db`, `WhatsApp/`, …). Default: process cwd. Not the extract cwd.

**--json** *PATH*
: Skip wtsexporter; convert an existing `result.json`.

**--key** *KEY|PATH*
: Crypt key file path or crypt15 hex string (forwarded as `-k`). Not saved by the GUI to `export.ini`.

**--backup** *PATH*
: Encrypted Android backup or iOS backup path (forwarded as `-b`). Required for iOS in the GUI.

**--wa** *PATH*
: Contacts DB `wa.db` / `ContactsV2.sqlite` (forwarded as `-w`).

**--media** *PATH*
: WhatsApp media folder (forwarded as `-m`).

**--db** *PATH*
: Explicit message database (forwarded as `-d`).

**--business**
: Use WhatsApp Business package defaults.

**--start-date** / **--end-date** *YYYY-MM-DD*
: Local date filter (inclusive start, exclusive end).

**--media-mode** *MODE*
: `disabled`, `clone` (default), `convert`, or `compress`.

**--media-max-resolution**, **--media-max-fps**, **--media-min-size**, **--media-skip-efficient**
: Compress-only knobs (defaults `1080p`, `30`, `20M`, `true`).

**--obfuscate**, **--obfuscate-seed** *8-hex*
: Post-export obfuscation; seed must be exactly eight hex digits.

# EXIT STATUS

Non-zero if `wtsexporter` is missing/fails, JSON is missing, convert fails, or media tools fail entirely. Spawn hints for broken pipx shims are printed on failure.

# FILES

**Output**
: `*__whatsapp.csv`, `attachments/`, `wtsexporter_result.json`. Scratch `wtsexporter-*` under output during the run.

**Upstream**
: Requires `wtsexporter` on `PATH`, beside this binary, in `MESSAGE_EXPORTERS_BIN`, or via `WTSEXPORTER`.

# ENVIRONMENT

**WTSEXPORTER**
: Absolute path to the `wtsexporter` binary.

**MESSAGE_EXPORTERS_BIN**
: Directory searched for `wtsexporter` / `wtsexporter.exe`.

**TQDM_DISABLE**
: Set to `1` by this tool when spawning wtsexporter (progress bars off).

**PATH**
: Needs `ffmpeg` / `ffprobe` for `convert` / `compress`; also used to find `wtsexporter`.

# EXAMPLES

```bash
# Android crypt15
whatsapp-exporter \
  --platform android \
  --key /path/to/key-or-hex \
  --backup msgstore.db.crypt15 \
  --output ./staging/whatsapp

# iOS backup
whatsapp-exporter \
  --platform ios \
  --backup ~/Library/Application\ Support/MobileSync/Backup/DEVICE_ID \
  --output ./staging/whatsapp

# Convert-only
whatsapp-exporter \
  --json /path/to/result.json \
  --output ./staging/whatsapp
```

# NOTES

Supported exporter. Install helper with `pip install 'whatsapp-chat-exporter[android_backup,crypt15]'` or use the release-bundled binary. Prefer this over iMazing WhatsApp CSV when you have the native DB/backup.

# SEE ALSO

[README.md](../README.md),
[KnugiHK WhatsApp-Chat-Exporter](https://github.com/KnugiHK/WhatsApp-Chat-Exporter),
[imazing-exporter](../../imazing-exporter) (CSV path),
[message-media](../../message-media), [message-obfuscate](../../message-obfuscate)
