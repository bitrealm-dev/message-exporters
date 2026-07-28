# imessage-ir-exporter(1)

## Name

imessage-ir-exporter - export Apple Messages (chat.db) via common message to JSON/CSV/EML/MBOX/JSONL/XML

## Synopsis

```text
imessage-ir-exporter --output <DIR> [--format json|jsonl|csv|eml|mbox|xml] [options]
```

## Description

Reads Apple Messages from a macOS `chat.db` or an iOS backup, builds a [common message](../../../docs/src/common-message.md) ([`message-ir`](../../../docs/COMMON_MESSAGE.md)) per conversation, and projects JSON (default) / JSONL / CSV / EML / MBOX / XML.

## Options

| Flag | Description |
|------|-------------|
| `--input <PATH>` | `chat.db` (macOS) or iOS backup root (default: system Messages DB) |
| `--output <DIR>` | Output directory |
| `--format <FORMAT>` | `json` (default), `jsonl`, `csv`, `eml`, `mbox`, or `xml` |
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
imessage-ir-exporter --output ./staging/imessage
imessage-ir-exporter --format csv --copy-method clone --output ./staging/imessage
imessage-ir-exporter --format eml --platform iOS --input ~/Library/Application\ Support/MobileSync/Backup/<id> --output ./out
```
