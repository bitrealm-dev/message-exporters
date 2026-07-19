# iMazing exporter design and limitations

Living design notes for [`imazing-out`](../). Append dated findings; do not erase prior validation rows.

**Targeted upstream:** iMazing **3.5.5** (validated against a full “All messages” device export on 2026-07-19).

## Goals

- Accept either one iMazing Messages/WhatsApp CSV **or** a folder at any level of a device export tree.
- Emit vault-shaped per-conversation CSV, with WhatsApp kept separate from SMS/iMessage.
- Resolve phones/names through an optional iMazing Contacts CSV.
- Document export limitations that cannot be fixed in the converter.

## Input layout (verified)

A full device export root typically contains:

```text
Device-Info.txt
Contacts/.../Contacts - {stamp}.csv
Messages/{YYYY-MM-DD HH MM SS} - {label}/Messages - {export-stamp} - {label}.csv
WhatsApp/{YYYY-MM-DD HH MM SS} - {label}/WhatsApp - {export-stamp} - {label}.csv
```

Media files sit beside the CSV in the chat folder (no `Attachments/` subdir). Disk names are often
`{msgTs} - {truncatedLabel} - {originalBasename}`; the CSV `Attachment` cell is usually only the original basename.

`--input` may be:

| Path | Behavior |
|------|----------|
| One `.csv` | Parse if headers match Messages or WhatsApp |
| Chat folder | Recurse; pick matching CSV(s) |
| `Messages/` or `WhatsApp/` | Recurse that tree |
| Device export root | Recurse; skip Contacts CSVs by name/header |

Discovery is recursive, does not follow directory symlinks, sorts paths, and classifies by headers:

- **Messages:** has `Service` (plus shared `Chat Session` / `Message Date` / `Sender ID`)
- **WhatsApp:** lacks `Service`; has `Forwarded`, `Attachment info`, and/or `Sent Date`

## Schemas (verified headers)

### Messages (17 columns)

```text
Chat Session, Message Date, Delivered Date, Read Date, Edited Date, Deleted Date,
Service, Type, Sender ID, Sender Name, Status, Replying to, Subject, Text, Reactions,
Attachment, Attachment type
```

`Service` values observed: `SMS`, `iMessage` (sometimes mixed in one chat).

### WhatsApp (14 columns)

```text
Chat Session, Message Date, Sent Date, Type, Sender ID, Sender Name, Status, Forwarded,
Replying to, Text, Reactions, Attachment, Attachment type, Attachment info
```

Canonical output `service` is always `WhatsApp`.

### Contacts

Wide address-book CSV (`First Name`, `Mobile Phone`, …, `Notes`). Phones may appear only in
`Notes` as `PROP-ID: +…` (handled by `message-contacts`).

## Output policy

- SMS + iMessage for the same peer merge into one conversation (Messages family).
- WhatsApp for the same peer is a **separate** CSV (`…__whatsapp.csv`).
- Notification rows keep `imazing_type=Notification`; direction is emitted as `incoming`.
- Vendor-lossy fields are preserved as columns: `imazing_type`, `replying_to`, `forwarded`,
  `attachment_info`, `delivered_date`, `read_date`, `edited_date`, `deleted_date`, `sent_date`.
- Deduplication key includes attachment identity so same-time/text with different media are kept.
- Attachments are recorded by CSV filename only in this version (no media copy / suffix join yet).
- Very large group chat ids are written to short `group_{hash16}.csv` filenames on disk (Windows
  path limits); the full `chat_identifier` cell is unchanged.

## Chat identity and participants

### Individual chats

Prefer `Sender ID` phones/emails; else normalize a phone-like `Chat Session`; else Contacts
name→phone; else a sanitized name stem (reported as unresolved).

### Messages groups

`Chat Session` often encodes a roster as `Name A & Name B & Name C`.

1. Collect phones/emails from sender rows and `+digits` in the session string.
2. Split roster labels on ` & `.
3. Resolve name-only labels through Contacts.
4. Group `chat_identifier` = sorted, comma-joined resolved handles when any exist.

### WhatsApp groups

`Chat Session` is a **title**, not a roster. Participants are inferred only from distinct senders.
Non-senders are invisible in the CSV.

## Confirmed limitations

These are upstream/export constraints, not converter bugs:

1. **Silent Messages group member has no phone** unless their display name in `Chat Session`
   resolves through Contacts. If they never send and are absent from Contacts, only the name
   exists in the roster string.
2. **WhatsApp non-senders are absent** — no roster column; Notifications rarely include phones.
3. **Outgoing identity blank** — own number never appears in `Sender ID` / `Sender Name`.
4. **Name-only chat sessions** — many 1:1 chats use a display name as `Chat Session`.
5. **Timezone-less timestamps** — `Message Date` is naive; importer requires `--timezone` or host local.
6. **Folder/label truncation** — long group folder names may end mid-name with `-`.
7. **Attachment rename mismatch** — CSV basename ≠ on-disk filename; converter does not yet
   join by suffix or copy media.
8. **Contact phone gaps** — many contacts lack phone columns; some phones only in Notes.
9. **Replies / reactions are free text** — not structured objects; reaction times use US `M/D/YYYY`.
10. **Edited / deleted** — rare Messages columns / `Recently deleted` status; preserved as raw fields.
11. **No group GUID** — only display strings and inferred handles.
12. **Email iMessage chats** — uncommon; `Sender ID` may be an email.

## Validation matrix

| Date | iMazing | Sample | Result |
|------|---------|--------|--------|
| 2026-07-19 | 3.5.5 | Full device export (Messages + WhatsApp + Contacts) | Headers/layout confirmed; silent-roster limitation quantified on Messages groups; WhatsApp schema differs as above |
| 2026-07-19 | 3.5.5 | Synthetic fixtures in `tests/fixtures/` | Recursive discovery, service separation, silent-member contact recovery |

## Future work (not yet implemented)

- Copy/link attachments by suffix-matching disk filenames in the chat folder.
- Optional owner-phone flag to annotate outgoing sender handle.
- Structured parse of reactions / replies if a stable grammar is confirmed.
- Emit `participants_json` when vault ingest expects it.

## Related docs

- Converter README: [`../README.md`](../README.md)
- Contacts helper: [`../../message-contacts/README.md`](../../message-contacts/README.md)
