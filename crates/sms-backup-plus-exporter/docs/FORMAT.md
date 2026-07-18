# SMS Backup+ EML format notes

Input messages come from [SMS Backup+](https://github.com/jberkel/sms-backup-plus) syncing Android SMS/MMS to Gmail/IMAP, then archived as `.eml` (this project does **not** talk to IMAP).

Reference parsing: message-vault `sms_eml_master` (`flat_eml/eml_parse.py`, `archive_parse.py`).

## Flat single-message EML

Typical headers:

| Header | Meaning |
|--------|---------|
| `X-smssync-type` | Android SMS type; sent ≈ `{2,128,4,135,6,5}`, received ≈ `{1,132,130}` |
| `X-smssync-address` | Counterparty phone(s); groups use `~` (or `;`, `,`, `\|`) separators |
| `X-smssync-date` | Java epoch **milliseconds** (or seconds if small) |
| `X-smssync-id` | Stable sync id (optional) |
| `Subject` | `SMS with {contact name}` |
| `From` / `To` | Often `*@sms-backup-plus.local` or owner Gmail |

Body is `text/plain` (first part). Non-text MIME parts are exported as attachments.

## Archive EML

| Header / body | Meaning |
|---------------|---------|
| `Subject` | `SMS archive {contact name}` |
| `From` | Often `{digits}@sms-backup-plus.local` |
| Body lines | `YYYY-MM-DD HH:MM:SS - {Sender}` then message text; Sender `Me` = sent |

Optional MIME attachments are attached to messages in order.

## Mapping to SMS NDJSON (`message_json::sms`)

| Source | SMS NDJSON |
|--------|------------|
| counterparty E.164 / group key | `chat_identifier` |
| — | `service` = `"SMS"`, `"schema": "sms"`, `schema_version` = `2`, `conversation_type` |
| X-smssync-type / From owner email | `is_from_me` |
| address / From | `sender` |
| body | `text` |
| MIME parts | `attachments[]` under `attachments/` |

## Authoritative flat EML dedupe (`dedupe-eml`)

Identity for unique flat files:

1. When `X-smssync-id` is present: `smssync:{id}|{chat_id}|{timestamp_ms}|{is_from_me}|{normalized_text}`
   (bare ids alone are not unique across devices / reinstalls)
2. Else `chat_id | timestamp_ms | is_from_me | normalized_text | sha256(attachment bytes)…`

Text normalization collapses whitespace runs. Flat dedupe keeps millisecond precision.

Archive coverage uses a **cover key**: chat + time floored to whole seconds + direction + normalized text (no attachment hashes). That way archive body timestamps (`YYYY-MM-DD HH:MM:SS`) match flat `X-smssync-date` values that include milliseconds. Archive body timestamps are interpreted as local wall-clock time.

Output filenames: `{YYYY}/{YYYYMMDD_HHMMSS}_{sent|recv}_{chat_key}_{short_id}.eml` (`chat_key` is E.164 without a leading `+`).

Archive-only messages (present in an archive thread, no matching flat) are **generated** as SMS Backup+–shaped flats (`GENERATED` in `dedupe.log`): synthetic `X-smssync-id` (`gen-{hash}`), minimal `Date`/`Message-ID`/From/To/`X-smssync-*` headers (no fake Gmail `Received` or other export-tool metadata). Unknown-phone flats and archives are written under `junk/{YYYY}/`. Unparseable SMS-shaped (or broken) `.eml` files are copied to `unparseable/` beside `junk/`. Name-mapping and contact-resolution summary counts are unique identities, not per-message hits.
