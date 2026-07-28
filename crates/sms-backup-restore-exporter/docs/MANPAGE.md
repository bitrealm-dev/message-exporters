# NAME

sms-backup-restore-exporter - convert SMS Backup & Restore XML via common message to JSON/CSV/EML/MBOX/JSONL/XML

# SYNOPSIS

```text
sms-backup-restore-exporter --input <PATH> --output <DIR> --owner-phone <PHONE>...
    [--format json|jsonl|csv|eml|mbox|xml]
    [--contacts <PATH> | --vcf <PATH>]
    [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
    [--media-mode disabled|clone|convert|compress]
    [--media-max-resolution 720p|1080p|4k] [--media-max-fps <N>]
    [--media-min-size <SIZE>] [--media-skip-efficient true|false]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

# DESCRIPTION

Converts SyncTech **SMS Backup & Restore** XML into a common message per conversation, then projects JSON (default), JSONL, CSV, EML, MBOX, or SyncTech XML (`--format`). See [common-message.md](../../../docs/src/common-message.md), [COMMON_MESSAGE.md](../../../docs/COMMON_MESSAGE.md), and [MAIL_ARCHIVE.md](../../../docs/MAIL_ARCHIVE.md). MMS media is written under `attachments/` when enabled; for EML/MBOX, attachment bytes are embedded.

Owner phone(s) are required so MMS chat keys and direction resolve correctly. Encrypted ZIP backups must be unlocked/extracted before use. Media convert/compress and obfuscation apply through FormatSink for every format.

# OPTIONS

**--input** *PATH*
: An `sms-*.xml` file, or a directory of `.xml` files.

**--output** *DIR*
: Destination for packaging output and `attachments/`.

**--format** *json|jsonl|csv|eml|mbox|xml*
: Output packaging from the common message. `json` (default) one common-message file per conversation; `jsonl` lines; `csv` one CSV per conversation; `eml` / `mbox` mail archives; `xml` one `smses.xml`.

**--owner-phone** *PHONE*
: Owner number (E.164 or digits). Repeat for multiple. Required.

**--contacts** *PATH*
: Contacts file (VCF or iMazing Contacts CSV). Optional.

**--vcf** *PATH*
: Contacts VCF (alternate to `--contacts`). At most one of `--contacts` / `--vcf`.

**--start-date** *YYYY-MM-DD*
: Include messages on or after this local date (inclusive).

**--end-date** *YYYY-MM-DD*
: Include messages before this local date (exclusive).

**--media-mode** *MODE*
: `disabled`, `clone` (default), `convert`, or `compress`. Convert/compress need ffmpeg/ffprobe.

**--media-max-resolution** *RES*
: Compress only: max long edge (`720p`, `1080p` default, `4k`).

**--media-max-fps** *N*
: Compress only: max frame rate (default `30`).

**--media-min-size** *SIZE*
: Compress only: re-encode videos at/above this size (default `20M`).

**--media-skip-efficient** *true|false*
: Compress only: skip efficient HEVC under max resolution (default `true`).

**--obfuscate**
: Rewrite names, numbers, text, and media with stable fakes after export.

**--obfuscate-seed** *8-hex*
: Exactly eight hexadecimal characters; implies `--obfuscate`.

# EXIT STATUS

Exits non-zero on invalid arguments, missing input, convert failure, or total media-tool failure. Progress/warnings on stderr; summary on stdout.

# FILES

**Input**
: SyncTech XML with embedded or referenced MMS parts.

**Output**
: With `--format json` (default): one `*.json` per conversation; `attachments/` for copied MMS media. With `--format csv`: one `*.csv` per conversation. With `--format eml`: one directory per conversation of `*.eml` files.

# ENVIRONMENT

**PATH**
: Must include `ffmpeg` and `ffprobe` for `convert` / `compress`.

# EXAMPLES

```bash
sms-backup-restore-exporter \
  --input /path/to/sms-20210328165031.xml \
  --output ./staging/sms-backup-restore \
  --owner-phone +15555550100 \
  --contacts /path/to/contacts.csv
```

# NOTES

Supported exporter (documented XML schema). Call logs, drafts, failed, and queued messages are skipped. See [FIELDS.md](FIELDS.md) and [XML_CSV_MAPPING.md](XML_CSV_MAPPING.md).
