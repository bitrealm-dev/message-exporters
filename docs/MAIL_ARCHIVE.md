# Mail archive format (EML)

Design for a human-viewable export: **one folder per conversation**, **one `.eml` per message**, with structured `X-ME-*` headers for machine fidelity. Intended as an archive / interchange path before vault exists. Mail clients can open individual messages; translators can recover SMS, group MMS, and (later) iMessage semantics without relying on CSV.

**Status:** specification only. No writer crate or CLI/`OutputFormat` flag yet. Current exporters still write per-conversation CSV ([csv-output.md](src/csv-output.md)).

## Goals

- Browseable archive (double-click an `.eml`; folder = conversation)
- Lossless-enough SMS and group MMS relative to today’s SBR / GO SMS Pro / SMS Backup+ CSV cores
- Stable `Message-ID` / guid for re-exports and threading
- Metadata in `X-ME-*` headers (not only human body text)
- Room for iMessage (tapbacks, balloons, parts, edits) without colliding with the SMS model

## Non-goals (this document)

- Vault import/export
- Replacing CSV as the default exporter output
- GUI / CLI format switches
- IMAP sync or SMS Backup+ wire compatibility
- Writing `.mbox` as the canonical form (optional derived export later)
- Replaying send-effect animations or handwriting ink in clients

## Packaging

```text
output/
  <conversation-stem>/
    000001_<yyyy-mm-dd>_<hhmmss>_<guid8>.eml
    000002_<yyyy-mm-dd>_<hhmmss>_<guid8>.eml
    ...
```

- **Conversation stem:** same rules as CSV filenames from [`message-csv::conversation_filename`](../crates/message-csv/src/lib.rs), without the `.csv` suffix (e.g. `+15555550101`, `Family_Chat`, `group_+A_+B`).
- **Sequence prefix:** zero-padded decimal in chronological emit order so file browsers sort stably.
- **Timestamp in name:** local wall-clock of the message for skimming (authoritative time is still `Date` / `X-ME-Timestamp-Unix-Ms`).
- **`guid8`:** first 8 hex chars of `X-ME-Guid` (or Message-ID local-part hash) to avoid collisions when two messages share a second.

Each file is one RFC 5322 message. Prefer writing via a MIME builder (e.g. `mail-builder`) when implemented.

### Why not one `.mbox` per conversation (canonical)

| Concern | Folder of `.eml` | Single `.mbox` |
|---------|------------------|----------------|
| Translate / reprocess | One message = one file | Must parse mbox + `From ` escaping |
| Crash safety | Partial export remains usable | Truncation can corrupt the last record |
| Plus anti-patterns | One message owns its MIME parts | Easier to regress into “fat archive” |
| Large chats | Open one message | Some clients load the whole file |
| Thunderbird “mailbox” UX | Import/drag varies | Often smoother import-as-folder |

**Canonical form = folder of EMLs.** A future tool may synthesize `mboxrd` from that folder for clients that prefer mailbox import. Outlook has poor native support for both; do not optimize the canonical format for Outlook.

### Explicit anti-pattern: SMS Backup+ archive EML

Do **not** pack many SMS into one MIME body with order-based attachment assignment (SMS Backup+ `Subject: SMS archive …`). Perfect pairing is impossible; see [Plus FORMAT.md](../crates/sms-backup-plus-exporter/docs/FORMAT.md) and `archive.rs` heuristics.

## Lessons from SMS Backup+ (do not repeat)

| Pitfall | Instead |
|---------|---------|
| Multi-message archive EML + FCFS attachment leftover assignment | One `.eml` per message; each MIME part belongs to that message only |
| `*@sms-backup-plus.local` as sole identity | Synthetic addresses with E.164 in the local-part **and** `X-ME-*` handles |
| Chat keyed to owner when address is `owner~peer` | First non-owner peer / full roster in `X-ME-Participants` |
| Archive body times as ambiguous local wall-clock | `Date` in UTC (RFC 5322) + `X-ME-Timestamp-Unix-Ms` |
| Opaque Android type ints alone | Clear `X-ME-Direction` / `X-ME-Message-Kind` (+ optional `X-ME-Android-Type`) |
| Group archives with no roster | Always emit `X-ME-Participants` for groups |
| Dedupe / identity via `X-smssync-id` alone | Stable `X-ME-Guid` / `Message-ID` from content fingerprint or source guid |
| Free-text-only reactions (iMazing-style) | Structured tapback EMLs + headers (see iMessage section) |

Do **not** use the `X-smssync-*` header namespace. This format is not Plus-compatible.

## Container model (every message)

### Standard mail headers

| Header | Rule |
|--------|------|
| `From` / `To` / `Cc` | Human-readable mapping (below); synthetic `+E164@sms.local` or handle-based local-part |
| `Date` | Message timestamp as RFC 5322 **UTC** |
| `Subject` | Short preview (peer name, group title, or SMS subject); not the sole carrier of semantics |
| `Message-ID` | Stable, unique, deterministic (see below) |
| `MIME-Version` | `1.0` |
| `Content-Type` | `text/plain` or `multipart/mixed` (or `multipart/related`) when attachments exist |
| `In-Reply-To` / `References` | Set for iMessage replies and tapbacks; **unset** for ordinary SMS |

### Synthetic addresses

- Phone: `+15551234567@sms.local` (E.164 in local-part; `+` allowed in addr-spec via quoting if required by the builder).
- Email / Apple handle: `user=example.com@handle.local` or a documented safe encoding of the raw handle — never name-only as the sole identifier.
- Display name may appear in the phrase (`Alice <+1555…@sms.local>`).

### Message-ID

- Prefer source guid when present (iMessage): `<{apple-guid}@imessage.local>`.
- Otherwise: `<{sha256-fingerprint}@message-exporters.local>` matching CSV `guid` construction where possible.
- Must be stable across re-exports of the same logical message.

### From / To / Cc mapping

**1:1 incoming:** `From` = peer; `To` = owner.

**1:1 outgoing:** `From` = owner; `To` = peer.

**Group incoming:** `From` = actual sender; `To`/`Cc` = other participants (including owner); full roster still in `X-ME-Participants`.

**Group outgoing:** `From` = owner; `To` = all other participants; roster in `X-ME-Participants`.

Outgoing `X-ME-Sender-Handle` is empty/absent (same convention as CSV).

## Core `X-ME-*` headers (SMS / MMS / shared)

Prefix: **`X-ME-`** (Message Exporters). JSON header values are compact single-line JSON.

| Header | Values / shape | Notes |
|--------|----------------|-------|
| `X-ME-Chat-Identifier` | string | Same role as CSV `chat_identifier` |
| `X-ME-Conversation-Type` | `individual` \| `group` | |
| `X-ME-Group-Title` | string | Empty/absent for 1:1 |
| `X-ME-Participants` | JSON `[{ "handle", "display_name" }]` | **Required for groups**; E.164 preferred for phones |
| `X-ME-Direction` | `incoming` \| `outgoing` | |
| `X-ME-Sender-Handle` | string | Empty when outgoing |
| `X-ME-Sender-Display-Name` | string | |
| `X-ME-Service` | `SMS` \| `iMessage` \| `RCS` | Android SMS path uses `SMS` even for MMS |
| `X-ME-Message-Kind` | see taxonomy below | |
| `X-ME-Timestamp-Unix-Ms` | integer string | Authoritative epoch ms (UTC) |
| `X-ME-Timestamp-Display-TZ` | optional offset/name | When export used a non-host timezone |
| `X-ME-Subject` | string | When distinct from mail `Subject` |
| `X-ME-Guid` | hex / guid string | Matches CSV `guid` when possible |
| `X-ME-Export-Source` | string | e.g. `sms-backup-restore` |
| `X-ME-Export-Tool` | string | |
| `X-ME-Export-Tool-Version` | string | |
| `X-ME-Android-Type` | integer string | Optional; SMS `type` / MMS `msg_box` |
| `X-ME-Source-Fields` | JSON | Optional full-fidelity bag (CSV `xml_fields_json` / PDU extras) |
| `X-ME-Attachment-Meta` | JSON array | Parallel to MIME attachment parts (see Attachments) |

### Message-kind taxonomy (shared)

| Kind | Meaning |
|------|---------|
| `sms` | SMS text |
| `mms` | MMS (may include media) |
| `imessage` | Normal iMessage text/media |
| `tapback` | Reaction row |
| `sticker_tapback` | Sticker used as reaction |
| `balloon` | App / URL / poll / Digital Touch / etc. |
| `announcement` | Group system message |
| `location_share` | Live location start/stop |

SMS writers use `sms` / `mms` only. Absence of iMessage-only headers means “not applicable,” not an empty array.

## Group MMS rules

1. Emit `X-ME-Participants` with every non-empty handle (sorted stably for hashing if needed).
2. Never drop the roster because `From`/`To` already list some addresses.
3. Incoming sender must be the real sender handle when known (not an arbitrary group member).
4. Untitled groups: stem from sorted participant phones (same as CSV); title may still be empty.

## Attachments

**v1: embed bytes as MIME parts** so offline mail clients show media.

- `Content-Type` from known mime; fallback `application/octet-stream`
- `Content-Disposition: attachment; filename="…"` using original name when known
- Part order is significant: index `0..n-1` of non-body MIME attachments matches `X-ME-Attachment-Meta` and iMessage `parts[].attachment_indices`

`X-ME-Attachment-Meta` JSON array (CSV `AttachmentCell`-aligned):

```json
[
  {
    "path": null,
    "original_name": "IMG_001.jpg",
    "mime_type": "image/jpeg",
    "is_sticker": false,
    "transcription": null,
    "sticker_effect": null,
    "digest_sha256": "…"
  }
]
```

- `path` may be null when bytes are embedded only; digest supports dedupe across re-exports.
- **Never** assign leftover MIME parts to the “last” message in a conversation (Plus archive anti-pattern).

Optional later mode: external files under `attachments/` with `Content-Location` — not required for v1.

## Body text

- Primary human body: `text/plain; charset=utf-8` (UTF-8).
- For multipart messages: first body part is flattened readable text; media follows as attachments.
- Placeholders such as `[attachment]` / `[app]` are acceptable when flattening iMessage parts.
- Optional `text/html` is deferred.

---

## iMessage extension

Align with the iMessage CSV inventory in [`crates/imessage-exporter/src/exporters/csv/`](../crates/imessage-exporter/src/exporters/csv/). SMS exporters MUST NOT emit these headers.

### Threading (replies)

| Header | Role |
|--------|------|
| `Message-ID` | `<{apple-guid}@imessage.local>` |
| `In-Reply-To` / `References` | Originator `Message-ID` |
| `X-ME-Is-Reply` | `true` |
| `X-ME-Thread-Originator-Guid` | Apple guid of thread root |
| `X-ME-Thread-Originator-Part` | Part index within multipart bubble |
| `X-ME-Num-Replies` | On originator when known |

Ordinary SMS leaves reply headers unset (no fake threads).

### Tapbacks / reactions

Apple stores tapbacks as separate `message` rows (`associated_message_type` 2000–2005 add, 3000–3005 remove, plus sticker associations).

**Canonical form: one `.eml` per tapback.**

| Header | Values |
|--------|--------|
| `X-ME-Message-Kind` | `tapback` \| `sticker_tapback` |
| `In-Reply-To` / `References` | Parent message `Message-ID` |
| `X-ME-Associated-Guid` | Parent Apple guid |
| `X-ME-Associated-Part` | Part index reacted to |
| `X-ME-Tapback-Kind` | `loved` \| `liked` \| `disliked` \| `laughed` \| `emphasized` \| `questioned` \| `emoji` \| `sticker` \| `removed_loved` \| … |
| `X-ME-Tapback-Emoji` | Custom emoji when present |
| `X-ME-Tapback-Action` | `add` \| `remove` |

Body `text/plain`: short human line (`Loved a message`, `😂 reacted`) so clients show something without parsing headers.

Sticker tapback: include sticker image MIME part + `X-ME-Attachment-Meta` with `is_sticker: true`.

**Optional aggregate on parent** (translator cache only):

```http
X-ME-Tapbacks: [{"part_index":0,"kind":"loved","reactor_handle":"+1555…","reactor_display_name":"Alex"}]
```

Readers SHOULD prefer per-message tapback EMLs. Do **not** store reactions only as free text in the parent body.

### Multipart bubbles (`X-ME-Parts`)

```http
X-ME-Parts: [{"index":0,"kind":"text","text":"Hi","attachment_indices":[],"effects":[]},{"index":1,"kind":"attachment","attachment_indices":[0],"effects":[]}]
```

Aligned with CSV `PartRecord`: `index`, `kind` (`text` \| `attachment` \| `app` \| `retracted` \| …), `text?`, `attachment_indices[]`, `effects[]`, `emoji_image?`.

MIME: `multipart/mixed` (or `related`) with flattened `text/plain` first, then attachments in emit order. Text effects (mention, link, styles, animated) stay in `parts[].effects` for v1 (no HTML reconstruction required).

### Edits / unsends

- Body = **current** visible text (empty if unsent).
- `X-ME-Edits`: JSON array aligned with CSV `EditEventRecord`: `{ part_index, status, text, timestamp?, timestamp_utc?, guid? }` with `status` ∈ `original` \| `edited` \| `unsent`.
- `X-ME-Is-Deleted: true` when tombstoned/deleted in DB.
- Do not invent separate “edit event” EMLs.

### Send effects

- `X-ME-Send-Effect` — same label space as CSV `send_effect` / `expressive_label` (slam, loud, gentle, invisible ink, screen effects, …).
- Not rendered by mail clients.

### Balloons / app messages

First-class messages: `X-ME-Message-Kind: balloon`.

| Header | Role |
|--------|------|
| `X-ME-Balloon-Bundle-Id` | Raw `balloon_bundle_id` |
| `X-ME-Balloon-Kind` | `url` \| `apple_pay` \| `poll` \| `handwriting` \| `digital_touch` \| `slideshow` \| `check_in` \| `find_my` \| `fitness` \| `business` \| `application` \| … |
| `X-ME-App` | JSON matching CSV `app_json` / `build_balloon_value` |

MIME:

- `text/plain` summary for preview (URL title, `Poll: …`, `Apple Pay`, …)
- Optional `application/json` part (`name=app.json`) when the payload is large
- Handwriting / Digital Touch: attach a rendered image/PDF **if** the exporter materializes one; otherwise bundle id + JSON only

### Announcements and location

- Announcement: `X-ME-Message-Kind: announcement`, `X-ME-Announcement: <text>`, body = same text
- Location: `X-ME-Message-Kind: location_share`, `X-ME-Shared-Location: started|stopped`, body may include map URL/text

### Stickers (message attachments)

Normal sticker sends: image MIME part + `X-ME-Attachment-Meta` (`is_sticker`, `sticker_effect?`, `transcription?`). Genmoji/memoji use the same path.

### Read receipts and participants

- `X-ME-Read-Receipt` — RFC 3339 when known
- `X-ME-Participants` — required for iMessage groups (full Apple roster)

### Client vs translator surfaces

| Surface | Mail client sees | Translator uses |
|---------|------------------|-----------------|
| Text / media | Body + MIME parts | same |
| Tapback | Short reply-like message | `X-ME-Tapback-*` + association |
| Balloon | Summary (+ optional image/JSON part) | `X-ME-App` |
| Edits | Current text only | `X-ME-Edits` |
| Effects | Nothing | `X-ME-Send-Effect` |
| Parts / mentions | Flattened text | `X-ME-Parts` |

---

## Mapping from today’s CSV cores (SMS)

| CSV column | Mail archive |
|------------|--------------|
| `chat_identifier` | `X-ME-Chat-Identifier` |
| `conversation_type` | `X-ME-Conversation-Type` |
| `group_title` | `X-ME-Group-Title` |
| `guid` | `X-ME-Guid` + `Message-ID` |
| `timestamp` / `timestamp_utc` / `date_ms` | `Date` + `X-ME-Timestamp-Unix-Ms` |
| `direction` | `X-ME-Direction` |
| `service` | `X-ME-Service` |
| `sender_handle` / `sender_display_name` | headers + `From` phrase |
| `subject` | `Subject` / `X-ME-Subject` |
| `text` | `text/plain` body |
| `attachments_json` | MIME parts + `X-ME-Attachment-Meta` |
| `message_kind` | `X-ME-Message-Kind` (`sms`/`mms`) |
| `android_type` | `X-ME-Android-Type` |
| `xml_fields_json` / PDU extras | `X-ME-Source-Fields` |
| `export_*` | `X-ME-Export-*` |
| `participants_json` (iMessage) | `X-ME-Participants` |
| `tapbacks_json` | tapback EMLs (+ optional `X-ME-Tapbacks`) |
| `parts_json` / `edits_json` / `app_json` | `X-ME-Parts` / `X-ME-Edits` / `X-ME-App` |
| `send_effect` | `X-ME-Send-Effect` |
| `thread_originator_*` | `In-Reply-To` + `X-ME-Thread-*` |

## Future implementation notes (non-binding)

1. Crate `message-mail` using `mail-builder` to emit one `.eml` per message into the conversation directory.
2. Shared intermediate struct mapped from exporter pending rows / iMessage CSV cell builders — not vault-specific.
3. First wiring candidate: **SMS Backup & Restore** (richest Android group MMS).
4. Second: **iMessage**, reusing `build_part_records`, `build_balloon_value`, tapback cells.
5. Later: optional `mboxrd` synthesis; `OutputFormat::{Csv, Eml}` on `ExporterConfig`.

## Related docs

- [CSV output conventions](src/csv-output.md)
- [Exporter capability matrix](EXPORTER_MATRIX.md)
- [SMS Backup+ EML input notes](../crates/sms-backup-plus-exporter/docs/FORMAT.md)
- [SBR XML → CSV mapping](../crates/sms-backup-restore-exporter/docs/XML_CSV_MAPPING.md)
