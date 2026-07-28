# Introduction

Message Exporters turns phone message backups into plain files a spreadsheet, mail program, or other tool can open. Each conversation becomes its own file (or folder), with photos and other media saved beside the messages when the chosen format needs them.

The app does not replace the tools that pull data off a phone. Use those tools (or a vendor backup) first, then run Message Exporters to normalize the result into a format that is easy to archive, search, or move elsewhere.

Every export builds a shared per-conversation **[common message](common-message.md)**, then packages it as the format you pick (default **JSON**). One Run is enough; Re-export is only for converting an existing output folder.

## Output formats

| Format | Best for |
|--------|----------|
| **JSON** / **JSON Lines** | Default packaging; machine-readable archives and later conversion |
| **CSV** | Spreadsheets, scripts, and vault-style import |
| **EML** | Reading threads in a mail client (one folder per chat) |
| **MBOX** | One mailbox file per chat |
| **XML** | One SyncTech `smses.xml` backup for restore-oriented workflows |

See [Common message](common-message.md) and [Choose an output format](formats.md).

## Supported backups

These paths are the recommended Message-tab sources:

1. **iPhone backup** — Apple Messages (`chat.db` / iOS backup)
2. **SMS Backup & Restore** — SyncTech XML
3. **WhatsApp** — native database or encrypted `crypt` backup (via `wtsexporter`)

Experimental converters (GO SMS Pro, iMazing CSV, OpenExtract, SMS Backup+) ship in the same release for cases where those are the only backups available. See [Experimental backups](exporters/experimental.md).

Already exported once? Use [Re-export between formats](reexport.md) to convert an existing output folder without re-reading the phone backup.

## Next steps

1. [Install](install.md) the desktop app from a GitHub Release.
2. [Use the desktop app](desktop-app.md) for a first successful export.
3. Open the guide for the backup type in hand under **Export**.
