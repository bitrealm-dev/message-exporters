# Introduction

Message Exporters turns phone message backups into plain files a spreadsheet, mail program, or other tool can open. Each conversation becomes its own file (or folder), with photos and other media preserved.

The app does not replace the tools that pull data off a phone. Use those tools (or a vendor backup) first, then run Message Exporters to build an archive in the output format of your choosing. Exported archives can be reprocessed into other archives if necessary.

## Output formats

| Format | Best for |
|--------|----------|
| **JSON** / **JSON Lines** | Default packaging; machine-readable archives and later conversion |
| **CSV** | Spreadsheets, scripts, and manual data inspection |
| **EML** / **MBOX** | Reading threads in a mail client |
| **XML** | Android-compatible format |

See [Common message](common-message.md) and [Choose an output format](formats.md).

## Supported backups

These paths are the recommended backup paths:

1. **iPhone backup** — Apple Messages (`chat.db` / iOS backup)
2. **SMS Backup & Restore** — Android messages
3. **WhatsApp** — native database or encrypted `crypt` backup (via `wtsexporter`)

Experimental converters (GO SMS Pro, iMazing CSV, OpenExtract, SMS Backup+) ship in the same release. They exist to rescue an existing backup when there is no way to get the data off the device again. They are not recommended otherwise: those source formats do not carry enough metadata to fully recreate the original messages, so conversion can be lossy and may rely on guesswork heuristics even with extra information supplied. See [Experimental backups](exporters/experimental.md).

Already exported once? Use [Re-export between formats](reexport.md) to convert an existing output folder without re-reading the phone backup.

## Next steps

1. [Install](install.md) the desktop app from a GitHub Release.
2. [Use the desktop app](desktop-app.md) for a first successful export.
3. Open the guide for the backup type in hand under **Export**.
