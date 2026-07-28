# Introduction

Message Exporters turns phone message backups into plain files a spreadsheet, mail program, or other tool can open. Each conversation becomes its own file (or folder), with photos and other media preserved.

The app does not replace the tools that pull data off a phone. Use those tools (or a vendor backup) first, then run Message Exporters to build an archive in the output format of your choosing. Exported archives can be reprocessed into other archives if necessary.

## Supported message exports

### iMessage / SMS

- **Apple** — one iPhone backup. The backup is complete: Messages (`chat.db`) and other app data come from that single archive.
- **Android** — XML from the SMS Backup & Restore app (the recommended path for SMS/MMS).

### WhatsApp

- **Apple** — the same iPhone backup used for iMessage / SMS.
- **Android** — an already extracted WhatsApp database. Getting that file off the phone is non-trivial; there is no one-size-fits-all method, so use whatever works for the device.

### Other app exports (experimental)

GO SMS Pro, iMazing CSV, OpenExtract, and SMS Backup+ ship in the same release. Use them only to rescue files another messaging app already wrote when you cannot get the data off the device again. Those formats do not carry enough metadata to fully recreate the original messages, so conversion can be lossy and may rely on guesswork. See [Experimental backups](exporters/experimental.md).

## Output formats

| Format | Best for |
|--------|----------|
| **JSON** / **JSON Lines** | Default packaging; machine-readable archives and later conversion |
| **CSV** | Spreadsheets, scripts, and manual data inspection |
| **EML** / **MBOX** | Reading threads in a mail client |
| **XML** | Android-compatible format |

See [Common message](common-message.md) and [Formats in detail](formats.md).

Already exported once? Use [Re-export between formats](reexport.md) to convert an existing output folder without re-reading the phone backup.

## Next steps

1. [Install](install.md) the desktop app from a GitHub Release.
2. [Use the desktop app](desktop-app.md) for a first successful export.
3. Open the guide for the backup type in hand under **Export**.
