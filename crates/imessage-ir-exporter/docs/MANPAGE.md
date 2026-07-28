# NAME

imessage-ir-exporter - export Apple Messages (chat.db / iOS backup) via common message to JSON/CSV/EML/MBOX/JSONL/XML

# SYNOPSIS

```text
imessage-ir-exporter --output <DIR>
    [--format json|jsonl|csv|eml|mbox|xml]
    [--input <PATH>] [--platform macOS|iOS|auto]
    [--copy-method clone|basic|full|disabled]
    [--attachment-root <PATH>] [--contacts <PATH>]
    [--backup-password <PASS>] [--conversation <ID>]
    [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
    [--use-caller-id true|false]
```

# DESCRIPTION

Reads Apple Messages from a macOS `chat.db` or an iOS backup via [`imessage-database`](https://crates.io/crates/imessage-database), builds a common message per conversation, and projects JSON (default), JSONL, CSV, EML, MBOX, or SyncTech XML.

The desktop GUI **iPhone backup** path uses this exporter for every output format. Media modes and obfuscate apply through the shared packaging pipeline. Library entrypoint: `imessage_ir_exporter::run` with `SourceConfig::Apple`.

License: GPL-3.0-or-later (same as `imessage-database` / `crabapple`).

# OPTIONS

**--input** *PATH*
: `chat.db` (macOS) or iOS backup root (default: system Messages DB).

**--output** *DIR*
: Output directory.

**--format** *json|jsonl|csv|eml|mbox|xml*
: Output packaging (`json` default).

**--platform** *macOS|iOS|auto*
: Platform detection / override.

**--copy-method** *clone|basic|full|disabled*
: Attachment handling (`clone` default).

**--attachment-root** *PATH*
: Custom attachment root (macOS).

**--contacts** *PATH*
: AddressBook / contacts path (macOS).

**--backup-password** *PASS*
: iOS backup password.

**--conversation** *ID*
: Limit to one chat identifier.

**--start-date** / **--end-date** *YYYY-MM-DD*
: Date filters.

**--use-caller-id** *true|false*
: Outgoing From display name (default `true`).

# EXAMPLES

```bash
cargo run -p imessage-ir-exporter -- --output ./staging/imessage

imessage-ir-exporter --format csv --copy-method clone --output ./staging/imessage

imessage-ir-exporter --format eml --platform iOS \
  --input ~/Library/Application\ Support/MobileSync/Backup/<id> \
  --output ./out
```
