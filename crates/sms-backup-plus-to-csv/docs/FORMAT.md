# SMS Backup+ EML format notes

Input messages come from [SMS Backup+](https://github.com/jberkel/sms-backup-plus) syncing Android SMS/MMS to Gmail/IMAP, then archived as `.eml` (this project does **not** talk to IMAP).

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

## CSV mapping and dedupe

EML → CSV field mapping and online cover-key dedupe: [`EML_CSV_MAPPING.md`](EML_CSV_MAPPING.md).
