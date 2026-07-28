# Choose an output format

Pick the packaging that matches the next tool in the workflow. Every Message-tab exporter and the Re-export tab can write the same set of formats.

## Quick chooser

| Need | Format |
|------|--------|
| Open chats in Excel, Numbers, or Google Sheets | **CSV** |
| Import into a vault or custom script that expects rows | **CSV** |
| Read threads in Apple Mail, Thunderbird, or Outlook | **EML** |
| One mailbox file per chat | **MBOX** |
| Keep a faithful machine archive for later conversion | **JSON** or **JSON Lines** |
| Produce a SyncTech-style `smses.xml` backup | **XML** |

## Formats in detail

### CSV

Writes one `.csv` file per conversation, a matching `.meta.json` header sidecar, and an `attachments/` folder when media is copied.

Use CSV for inspection, filtering, and tools that ingest tabular message data. Column meanings: [CSV columns](csv-output.md).

### EML

Writes one folder per conversation containing individual `.eml` messages.

Use EML when the goal is to browse history in a mail client. Media embeds according to attachment settings.

### MBOX

Writes one `.mbox` file per conversation.

Use MBOX when a single mailbox file per chat is easier to archive or import than a folder of EMLs.

### JSON

Writes one pretty-printed `.json` document per conversation (canonical IR). Attachment bytes live under `attachments/`, not inside the JSON.

Use JSON for round-trips and tooling that prefers a structured document per chat.

### JSON Lines (JSONL)

Writes one `.jsonl` file per conversation: a header line, then one message per line.

Use JSONL for streaming or line-oriented processing of large chats.

### XML

Writes a single `smses.xml` for the whole export (SyncTech SMS Backup & Restore shape).

Use XML when the next step expects that backup format. Apple-only fields are dropped. Prefer JSON or CSV when preserving iMessage detail matters.

## Change format later

Export once, then use [Re-export between formats](reexport.md) to produce another packaging from the same output folder without re-reading the phone backup.

## Next steps

1. Confirm the format in the Message or Re-export tab.
2. Continue with the [desktop app](desktop-app.md) or a source-specific export guide.
