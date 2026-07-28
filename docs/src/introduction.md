## Introduction

*message-exporters* converts message backups and existing export-tool output into a consistent, portable CSV format. It produces one file per conversation and saves attachments alongside the exported messages.

The project does not try to replace the specialist tools that extract data from phones and backups. Instead, it builds on their work: it wraps, normalizes, and converts their output so the result is easier to archive, search, transform, or use in another application.

## Motivation

Phone backups are made for restoring a device—not for letting people read or reuse their message history.

iCloud and Google backups generally cannot be browsed as an archive; they are meant to be restored to a phone. Apple’s local iTunes/Finder backups give users a copy of their data, but the messages still require separate tools to extract. On Android, complete application-data backups are usually unavailable without root, leaving users dependent on app-specific backup and export options.

Once messages are extracted, every tool has its own format and limitations. One may produce HTML, another XML, a database, or an incomplete export. Attachments, sender phone numbers, group participants, timestamps, and conversation titles may be missing or difficult to use. The output may look fine in a browser but be unsuitable for filtering, searching, analysis, custom rendering, or conversion to JSON or XML.

`message-exporters` avoids reinventing that extraction process. It provides a common CSV-based output that preserves the message data and available metadata from existing tools, giving users a format they can inspect and use however they want.

## Why CSV?

JSON is good for programs but difficult for humans to skim. HTML is easy to browse but awkward to sort, filter, or reuse as data.

CSV is the middle ground: plain text, easy to check, and supported by spreadsheets, scripts, databases, and archival tools. One row per message makes conversations simple to sort, filter, validate, convert, and import elsewhere—including message-vault-rs.

It is not meant to preserve every source-specific detail. It is a simple, portable common format that users can keep and use without relying on the original backup tool.

## Supported vs. Experimental

The desktop app focuses on 3 supported import paths:

1. **iPhone backups** — Apple Messages exports produced by `imessage-ir-exporter`
2. **SMS Backup & Restore** — SyncTech XML backups
3. **WhatsApp** — native databases and encrypted `crypt` backups processed by `wtsexporter`

Converters for:

- GO SMS Pro
- iMazing CSV
- OpenExtract
- SMS Backup+
   
are also included in the release ZIP, but are considered **experimental**. They are available for cases where those are the only backups you have, rather than recommended import methods.

