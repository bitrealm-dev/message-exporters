# Formats in detail

Pick the format that matches your needs. The Message tab and Re-export tab write the same set of formats; the default is **JSON**.

## Attachments

Attachment handling depends on the Attachments control (Copy / Convert / Compress / Do not copy). The trees below assume media copy is on. Do not copy skips media entirely. JSON / JSONL / CSV keep media in a sidecar `attachments/` folder; EML / MBOX / XML transform media then embed it and leave no sidecar.

## CSV

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

## EML

Writes one folder per conversation containing individual `.eml` messages.

**Attachments:** transformed, then embedded in each message (MIME parts). No sidecar folder.

**Layout:**

```text
output/
└── +15555550101/
    ├── 000001_2021-03-28_165031_a1b2c3d4.eml
    └── 000002_2021-03-28_170102_e5f6a7b8.eml
```

Use EML when the mail client imports a folder of individual message files.

**Note:** In a mail client, EML and MBOX both usually become one folder (or mailbox) per conversation with the messages inside. Pick whichever packaging your client imports best.

## MBOX

Writes one `.mbox` file per conversation.

**Attachments:** transformed, then embedded in the mailbox file (MIME parts). No sidecar folder.

**Layout:**

```text
output/
├── +15555550101.mbox
└── group_+15555550101_+15555550102.mbox
```

Use MBOX when the mail client prefers a single mailbox file per conversation.

**Note:** In a mail client, EML and MBOX both usually become one folder (or mailbox) per conversation with the messages inside. Pick whichever packaging your client imports best.

## JSON

Writes one pretty-printed `.json` document per conversation. See [Common message](common-message.md).

**Attachments:** sidecar `attachments/`; attachments are not byte encoded within the `.json`.

**Layout:**

```text
output/
├── +15555550101.json
└── attachments/
    └── IMG_0001.jpg
```

Use JSON as the default archive: one pretty-printed document per chat that is easy to open, inspect, and re-export. Prefer it over JSONL when you want the whole conversation as a single object.

## JSON Lines (JSONL)

Writes one `.jsonl` file per conversation: a header line, then one message per line.

**Attachments:** sidecar `attachments/`; attachments are not byte encoded within the `.jsonl`.

**Layout:**

```text
output/
├── +15555550101.jsonl
└── attachments/
    └── IMG_0001.jpg
```

Use JSONL to allow tools (i.e. `jq`, `grep`) to stream or filter message without loading the whole conversation at once.

## XML

Writes a single `smses.xml` for the whole export in SyncTech SMS Backup & Restore format. iMessage specific fields are dropped.

**Attachments:** transformed, then embedded as base64 inside `smses.xml`. No sidecar folder.

**Layout:**

```text
output/
└── smses.xml
```

Use XML for **Android compatibility**. Backing up or restoring a whole Android phone without third-party tooling requires root and often an unlocked bootloader, which is difficult on most devices. The SMS Backup & Restore app works without root, and `smses.xml` is the file it reads.

## Change format later

If you already have an export folder, use [Re-export between formats](reexport.md) to produce another packaging without re-reading the phone backup. A first export can write the target format directly—no Re-export step required.

## Next steps

1. Confirm the format in the Message or Re-export tab.
2. Continue with the [desktop app](desktop-app.md) or a source-specific export guide.
