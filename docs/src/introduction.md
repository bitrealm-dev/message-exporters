# Introduction

Backing up texts is easy. Getting the messages *out* in a form you can read is not.

Every backup app invents its own format—Android XML dumps, email archives, opaque PDU blobs, Apple’s Messages database. Those formats are built for restoring onto another phone, not for browsing history, searching, or keeping a durable archive.

**message-exporters** bridges that gap: turn vendor-specific backups into plain CSV—one spreadsheet file per conversation, with photos and other media saved beside those files.

## Why CSV?

JSON is great for programs, but hard for a person to skim. HTML is great in a browser, but awkward as a structured store you can sort, filter, or re-import.

CSV sits in the middle: human-readable enough to open and verify, structured enough for spreadsheets and downstream tools (including [message-vault-rs](https://github.com/bitrealm-dev/message-vault-rs)).

## Supported vs experimental

The desktop GUI and this site prioritize three **supported** sources:

1. **iPhone backup** (Apple Messages via `imessage-exporter`)
2. **SMS Backup & Restore** (SyncTech XML)
3. **WhatsApp** (native DB / crypt backup via `wtsexporter`)

Other converters (GO SMS Pro, iMazing CSV, OpenExtract, SMS Backup+) ship in the same release zip but are labeled **experimental**—useful when that is the only backup you have, not the recommended path.
