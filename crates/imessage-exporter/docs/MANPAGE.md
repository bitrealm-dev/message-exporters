# NAME

imessage-exporter - export Apple Messages (chat.db) to CSV, TXT, or HTML

# SYNOPSIS

```text
imessage-exporter -f csv|txt|html -o <DIR>
    [-c clone|basic|full|disabled] [-p <DB|BACKUP>]
    [-a <ATTACHMENT_ROOT>] [--platform macOS|iOS]
    [-s YYYY-MM-DD] [-e YYYY-MM-DD] [-t <FILTER>]
    [--use-caller-id] [--custom-name <NAME>]
    [--address-book <PATH>] [--cleartext-password <PASS>]
    [--obfuscate] [--obfuscate-seed <8-hex>]
    [other upstream options…]
```

Vault-oriented default used in this monorepo:

```text
imessage-exporter -f csv -c clone -o ./staging/imessage --use-caller-id
```

# DESCRIPTION

Reads Apple Messages history (`chat.db` on macOS, or an iOS backup root) and writes one file per conversation. In this repository the supported path is **CSV** with attachment copy (`-c clone`), matching the other vault exporters.

Fork of [ReagentX/imessage-exporter](https://github.com/ReagentX/imessage-exporter) with CSV export and post-export obfuscation. GPL-3.0-or-later.

# OPTIONS

Common flags for vault CSV export:

**-f**, **--format** *csv|txt|html*
: Output format. Use `csv` for vault-shaped spreadsheets.

**-c**, **--copy-method** *clone|basic|full|disabled*
: How attachments are handled. `clone` copies without converting (recommended here). `basic`/`full` convert media (may need ImageMagick/ffmpeg). Default if omitted is upstream’s default (often disabled)—pass `clone` explicitly for attachments beside CSV.

**-o**, **--export-path** *DIR*
: Output directory.

**-p**, **--db-path** *PATH*
: Path to `chat.db` (macOS) or iOS backup root. Default: platform Messages location.

**-a**, **--attachment-root** *PATH*
: Absolute override for attachment search (macOS / jailbroken layouts). Ignored for normal iOS backups.

**--platform** *macOS|iOS*
: Force platform detection.

**-s**, **--start-date** / **-e**, **--end-date** *YYYY-MM-DD*
: Inclusive start / exclusive end message filters.

**-t**, **--conversation-filter** *LIST*
: Comma-separated names, numbers, or emails to include.

**--use-caller-id**
: Use the owner’s caller ID instead of `"Me"` (GUI always enables this). Conflicts with `--custom-name`.

**--custom-name** *NAME*
: Custom display name for the owner’s messages. Conflicts with `--use-caller-id`.

**--address-book** *PATH*
: Optional AddressBook DB for handle→name mapping.

**--cleartext-password** *PASS*
: Password for encrypted iOS backups (visible in process list; omit to be prompted).

**--obfuscate**, **--obfuscate-seed** *8-hex*
: After CSV export, rewrite PII with stable fakes; seed must be exactly eight hex digits.

Also available (upstream): `--diagnostic`, `--no-lazy`, `--ignore-disk-warning`, `--no-progress`, and related flags. Run `imessage-exporter --help` for the full list.

# EXIT STATUS

Non-zero on missing database access, export failure, or invalid options. macOS may require Full Disk Access for the terminal/app reading Messages.

# FILES

**Input**
: `~/Library/Messages/chat.db` (typical macOS) or an iPhone backup directory.

**Output**
: Per-conversation files under `-o`; attachments copied when `-c clone` (or convert modes) is used.

# ENVIRONMENT

**HOME**
: Used for default database and output locations.

**PATH**
: May need ImageMagick/ffmpeg for non-clone copy methods on some platforms.

# EXAMPLES

```bash
# CSV + copied attachments (vault path)
imessage-exporter -f csv -c clone -o ./staging/imessage --use-caller-id

# Specific db and date range
imessage-exporter -f csv -c clone \
  -p /path/to/chat.db \
  -o ./staging/imessage \
  -s 2020-01-01 -e 2024-01-01 \
  --use-caller-id
```

# NOTES

Supported exporter in the desktop GUI (labeled **iPhone backup**). No `--owner-phone`; direction comes from the Messages database. Outgoing `sender_handle` is blank by design. See the crate README for sample CSV shape.

# SEE ALSO

[README.md](../README.md),
[upstream imessage-exporter](https://github.com/ReagentX/imessage-exporter),
[imazing-exporter](../../imazing-exporter) (iMazing CSV path),
[message-obfuscate](../../message-obfuscate)
