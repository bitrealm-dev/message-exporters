# NAME

sms-backup-plus-exporter - convert SMS Backup+ EML exports to per-conversation CSV

# SYNOPSIS

```text
sms-backup-plus-exporter [-v|--verbose] [--no-summary] convert
    --output <DIR>
    [--input <PATH>]...
    [--owner-phone <PHONE>]... [--owner-email <EMAIL>]...
    [--contacts <PATH> | --vcf <PATH>] [--name-mapping <PATH>]
    [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
    [--media-mode disabled|clone|convert|compress]
    [--media-max-resolution 720p|1080p|4k] [--media-max-fps <N>]
    [--media-min-size <SIZE>] [--media-skip-efficient true|false]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

# DESCRIPTION

Converts offline **SMS Backup+** `.eml` trees (IMAP-style `Archive/`, `Sent/`, …) into vault-shaped per-conversation CSV. Multiple `--input` roots are merged and path-deduped.

Owner phone and email may come from flags or `config/owner.toml` beside the crate. The GUI always runs the `convert` subcommand with `--verbose`.

# OPTIONS

## Global

**-v**, **--verbose**
: Log progress to stderr.

**--no-summary**
: Skip the end-of-run summary on stdout.

## convert

**--input** *PATH*
: An `.eml` file or directory of EMLs. Repeatable. Default: `source_dirs` from `config/owner.toml` when set. Exactly one path is required by the GUI.

**--output** *DIR*
: Destination for CSV and `attachments/`.

**--owner-phone** *PHONE*
: Owner number(s). Default: `phones` in `config/owner.toml`.

**--owner-email** *EMAIL*
: Owner email(s) for sent detection when `X-smssync-type` is missing. Default: `emails` in `config/owner.toml`.

**--contacts** *PATH*
: Contacts file (VCF or iMazing Contacts CSV). Optional.

**--vcf** *PATH*
: Contacts VCF (alternate to `--contacts`).

**--name-mapping** *PATH*
: CSV `Phone,Incorrect Name` for EML aliases. Default: `config/name-mapping.csv` when present.

**--start-date** / **--end-date** *YYYY-MM-DD*
: Local date filter (inclusive start, exclusive end).

**--media-mode** *MODE*
: `disabled`, `clone` (default), `convert`, or `compress`.

**--media-max-resolution**, **--media-max-fps**, **--media-min-size**, **--media-skip-efficient**
: Compress-only knobs (defaults `1080p`, `30`, `20M`, `true`).

**--obfuscate**, **--obfuscate-seed** *8-hex*
: Post-export obfuscation; seed must be exactly eight hex digits.

# EXIT STATUS

Non-zero on missing identity/input, convert errors, or total media-tool failure.

# FILES

**Input**
: Offline EML export tree (not live IMAP).

**Output**
: Per-conversation `*.csv`; `attachments/` when media is copied. Unresolved peers may land in `unknown.csv`.

**config/owner.toml**
: Optional defaults for phones, emails, and `source_dirs` (crate-relative when built from source).

# ENVIRONMENT

**PATH**
: Needs `ffmpeg` / `ffprobe` for `convert` / `compress` media modes.

# EXAMPLES

```bash
sms-backup-plus-exporter -v convert \
  --input /path/to/eml_export \
  --output ./staging/sms-backup-plus \
  --owner-phone +15555550100 \
  --owner-email you@example.com \
  --contacts /path/to/contacts.csv
```

# NOTES

Experimental in the GUI. Attachment→message pairing in archives is heuristic. See [FORMAT.md](FORMAT.md) and [EML_CSV_MAPPING.md](EML_CSV_MAPPING.md).

# SEE ALSO

[README.md](../README.md), [FORMAT.md](FORMAT.md), [EML_CSV_MAPPING.md](EML_CSV_MAPPING.md),
[message-contacts](../../message-contacts), [message-media](../../message-media),
[message-obfuscate](../../message-obfuscate)
