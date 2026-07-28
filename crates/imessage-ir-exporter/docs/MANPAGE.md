# imessage-ir-exporter(1)

## Name

imessage-ir-exporter - export Apple Messages (chat.db) to CSV, EML, MBOX, or JSON IR

## Synopsis

```text
imessage-ir-exporter --output <DIR> [--format csv|eml|mbox|json] [options]
```

## Description

Reads Apple Messages from a macOS `chat.db` or an iOS backup, builds canonical [`message-ir`](../../../docs/MESSAGE_IR.md) documents, and projects per-conversation CSV / EML / MBOX / JSON.

## Options

| Flag | Description |
|------|-------------|
| `--input <PATH>` | `chat.db` (macOS) or iOS backup root (default: system Messages DB) |
| `--output <DIR>` | Output directory |
| `--format <FORMAT>` | `csv` (default), `eml`, `mbox`, or `json` |
| `--platform <P>` | `macOS`, `iOS`, or `auto` |
| `--copy-method <M>` | `clone` (default), `basic`, `full`, or `disabled` |
| `--attachment-root <PATH>` | Custom attachment root (macOS) |
| `--contacts <PATH>` | AddressBook / contacts path (macOS) |
| `--backup-password <PASS>` | iOS backup password |
| `--conversation <ID>` | Limit to one chat identifier |
| `--start-date` / `--end-date` | `YYYY-MM-DD` filters |
| `--use-caller-id <bool>` | Outgoing From display name (default true) |

## Examples

```bash
imessage-ir-exporter --format json --output ./staging/imessage
imessage-ir-exporter --format csv --copy-method clone --output ./staging/imessage
imessage-ir-exporter --format eml --platform iOS --input ~/Library/Application\ Support/MobileSync/Backup/<id> --output ./out
```

## See also

[MESSAGE_IR.md](../../../docs/MESSAGE_IR.md), [MAIL_ARCHIVE.md](../../../docs/MAIL_ARCHIVE.md)
