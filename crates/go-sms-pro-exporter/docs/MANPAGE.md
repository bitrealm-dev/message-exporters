# NAME

go-sms-pro-exporter - convert GO SMS Pro XML+PDU backups to per-conversation CSV

# SYNOPSIS

```text
go-sms-pro-exporter --input <DIR> --output <DIR> --owner-phone <PHONE>...
    [--contacts <PATH> | --vcf <PATH>]
    [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
    [--media-mode disabled|clone|convert|compress]
    [--media-max-resolution 720p|1080p|4k] [--media-max-fps <N>]
    [--media-min-size <SIZE>] [--media-skip-efficient true|false]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

# DESCRIPTION

Reads a GO SMS Pro local backup folder (`gosms_sys*.xml` plus `I_*.pdu` files) and writes one vault-shaped CSV file per conversation under `--output`, with optional media under `attachments/`.

This binary is a thin CLI over the `go-sms-pro-exporter` library (`ExporterConfig` + `run`). The desktop GUI calls that library in-process; this command remains for standalone use.

Owner phone(s) are required: they determine message direction for PDU MMS. Wrong owner values flip sent/received.

# OPTIONS

**--input** *DIR*
: Backup folder containing XML and PDU files.

**--output** *DIR*
: Destination for per-conversation CSV and `attachments/`.

**--owner-phone** *PHONE*
: Owner number (E.164 or digits). Repeat for multiple owner numbers. Required.

**--contacts** *PATH*
: Contacts file for phone→name fill (VCF or iMazing Contacts CSV). Optional.

**--vcf** *PATH*
: Contacts VCF (alternate to `--contacts`). At most one of `--contacts` / `--vcf`.

**--start-date** *YYYY-MM-DD*
: Include messages on or after this local date (inclusive).

**--end-date** *YYYY-MM-DD*
: Include messages before this local date (exclusive).

**--media-mode** *MODE*
: `disabled` (no files), `clone` (default), `convert`, or `compress`. Convert/compress need ffmpeg/ffprobe.

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

Exits non-zero on invalid arguments, missing paths, convert failure, or when media convert/compress fails for all candidates. Warnings (e.g. missing contacts) go to stderr; a summary is printed to stdout on success.

# FILES

**Input**
: Directory with `gosms_sys*.xml` and matching `I_*.pdu` blobs.

**Output**
: One `*.csv` per conversation; `attachments/` when media mode copies files; optional `skipped_*.csv` diagnostics.

# ENVIRONMENT

**PATH**
: Must include `ffmpeg` and `ffprobe` when `--media-mode` is `convert` or `compress`.

# EXAMPLES

```bash
go-sms-pro-exporter \
  --input /path/to/gosms_export \
  --output ./staging/go-sms-pro \
  --owner-phone +15555550100 \
  --contacts /path/to/contacts.csv
```

# NOTES

Experimental in the desktop GUI. Proprietary PDU decoding is heuristic; many stub PDUs are empty. SMS attachments are not always present in the XML. Field mapping: [XML_CSV_MAPPING.md](XML_CSV_MAPPING.md).

# SEE ALSO

[README.md](../README.md), [XML_CSV_MAPPING.md](XML_CSV_MAPPING.md),
[message-contacts](../../message-contacts), [message-media](../../message-media),
[message-obfuscate](../../message-obfuscate)
