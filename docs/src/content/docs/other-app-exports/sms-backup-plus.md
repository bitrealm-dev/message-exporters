---
title: Rescue an SMS Backup+ export
description: Convert offline SMS Backup+ EML files and understand direction and attachment limits.
---

The SMS Backup+ importer targets version 1.5.11 EML layouts. It reads files already saved on disk. It does not sign in to Gmail, connect to IMAP, or download messages.

## Required input

- One `.eml` file or one directory of EML files in the desktop app.
- A separate output directory.
- Owner phone numbers and owner email addresses needed to identify sent messages.

Contacts are optional but recommended. Use a VCF or iMazing Contacts CSV. An optional name-mapping CSV has the columns `Phone,Incorrect Name`.

The command-line tool can accept more than one input root and can read defaults from `config/owner.toml`. The desktop app requires exactly one input path.

## Supported EML layouts

- **One message per file:** direction, peer, and time usually come from `X-smssync-type`, `X-smssync-address`, and `X-smssync-date`.
- **Archive email:** one email body contains many timestamped messages. A sender named `Me` is treated as outgoing.

## Run the import

In **Message**, choose **SMS Backup+ (experimental)**. Select the EML file or directory, enter owner phone and email identities, choose optional contacts or name mapping, then select the output format and directory.

## Known limitations

- Owner email is used for sent detection when `X-smssync-type` is absent. Missing identity can make direction uncertain or stop the import.
- Contacts are needed to resolve many name-to-phone relationships. Unresolved conversations can be written to `unknown`.
- Archive attachments are paired to messages by order, so attachment-to-message matching is best effort.
- Duplicate archive and individual-message copies are detected using conversation, timestamp rounded to the second, direction, and text. When they match, the individual-message copy is preferred and attachments are combined by file content.
- Source email layouts vary. Fields that SMS Backup+ did not write cannot be recovered.
