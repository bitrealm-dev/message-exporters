# Choose an output format

Pick the packaging that matches the next tool. The Message tab and Re-export tab write the same set of formats; the default is **JSON**.

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

Attachment handling depends on the Attachments control (Copy / Convert / Compress / Do not copy). The trees below assume media copy is on. **Do not copy** skips media entirely. JSON / JSONL / CSV keep media in a sidecar `attachments/` folder; EML / MBOX / XML transform media then **embed** it and leave no sidecar—the output folder is the archive. WhatsApp chats use a `__whatsapp` stem suffix (for example `+15555550101__whatsapp.json`).

### CSV

Writes one `.csv` file per conversation.

**Attachments:** sidecar `attachments/` folder; rows reference relative paths.

**Layout:**

```text
output/
├── +15555550101.csv
├── group_+15555550101_+15555550102.csv
└── attachments/
    └── IMG_0001.jpg
```

Use CSV for inspection, filtering, and tools that ingest tabular message data. Column meanings: [CSV columns](csv-output.md).

### EML

Writes one folder per conversation containing individual `.eml` messages.

**Attachments:** transformed, then embedded in each message (MIME parts). No sidecar folder.

**Layout:**

```text
output/
└── +15555550101/
    ├── 000001_2021-03-28_165031_a1b2c3d4.eml
    └── 000002_2021-03-28_170102_e5f6a7b8.eml
```

Use EML when the goal is to browse history in a mail client.

### MBOX

Writes one `.mbox` file per conversation.

**Attachments:** transformed, then embedded in the mailbox file (MIME parts). No sidecar folder.

**Layout:**

```text
output/
├── +15555550101.mbox
└── group_+15555550101_+15555550102.mbox
```

Use MBOX when a single mailbox file per chat is easier to archive or import than a folder of EMLs.

### JSON

Writes one pretty-printed `.json` document per conversation. See [Common message](common-message.md).

**Attachments:** sidecar `attachments/`; bytes are never inside the `.json` (documents reference paths and digests).

**Layout:**

```text
output/
├── +15555550101.json
└── attachments/
    └── IMG_0001.jpg
```

Use JSON as the default archive and for later re-packaging.

### JSON Lines (JSONL)

Writes one `.jsonl` file per conversation: a header line, then one message per line.

**Attachments:** sidecar `attachments/`; bytes are never inside the `.jsonl` (same as JSON).

**Layout:**

```text
output/
├── +15555550101.jsonl
└── attachments/
    └── IMG_0001.jpg
```

Use JSONL for streaming or line-oriented processing of large chats.

### XML

Writes a single `smses.xml` for the whole export (SyncTech SMS Backup & Restore shape).

**Attachments:** transformed, then embedded as base64 inside `smses.xml`. No sidecar folder.

**Layout:**

```text
output/
└── smses.xml
```

XML exists for **Android compatibility**. Backing up or restoring a whole Android phone without third-party tooling requires root and often an unlocked bootloader, which is difficult on most devices. The SMS Backup & Restore app works without root, and `smses.xml` is the file it reads—so this format is the practical way to move messages onto an Android phone.

Use XML when the destination is an Android device (or another tool that expects that backup format). Apple-only fields are dropped. Prefer JSON when preserving iMessage detail matters.

## Change format later

If you already have an export folder, use [Re-export between formats](reexport.md) to produce another packaging without re-reading the phone backup. A first export can write the target format directly—no Re-export step required.

## Next steps

1. Confirm the format in the Message or Re-export tab.
2. Continue with the [desktop app](desktop-app.md) or a source-specific export guide.
