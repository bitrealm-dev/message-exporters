# Choose an output format

Pick the packaging that matches the next tool. Every format is projected from the same [common message](common-message.md). The Message tab and Re-export tab share this set; CLI default is **JSON**.

## Quick chooser

| Need | Format |
|------|--------|
| Keep a faithful machine archive for later conversion (default) | **JSON** or **JSON Lines** |
| Open chats in Excel, Numbers, or Google Sheets | **CSV** |
| Import into a vault or custom script that expects rows | **CSV** |
| Read threads in Apple Mail, Thunderbird, or Outlook | **EML** |
| One mailbox file per chat | **MBOX** |
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

Writes one pretty-printed `.json` document per conversation — the common message on disk. Attachment bytes live under `attachments/`, not inside the JSON.

Use JSON as the default archive and for later re-packaging. See [Common message](common-message.md).

### JSON Lines (JSONL)

Writes one `.jsonl` file per conversation: a header line, then one message per line.

Use JSONL for streaming or line-oriented processing of large chats.

### XML

Writes a single `smses.xml` for the whole export (SyncTech SMS Backup & Restore shape).

XML exists for **Android compatibility**. Backing up or restoring a whole Android phone without third-party tooling requires root and often an unlocked bootloader, which is difficult on most devices. The SMS Backup & Restore app works without root, and `smses.xml` is the file it reads—so this format is the practical way to move messages onto an Android phone.

Use XML when the destination is an Android device (or another tool that expects that backup format). Apple-only fields are dropped. Prefer JSON when preserving iMessage detail matters.

## Change format later

If you already have an export folder, use [Re-export between formats](reexport.md) to produce another packaging without re-reading the phone backup. A first export can write the target format directly—no Re-export step required.

## Next steps

1. Confirm the format in the Message or Re-export tab.
2. Continue with the [desktop app](desktop-app.md) or a source-specific export guide.
