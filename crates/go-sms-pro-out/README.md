# GO SMS Pro → CSV

Convert a **GO SMS Pro** (GOMO / Jiubang) local backup into one spreadsheet file per conversation, plus any photos or other media found in the backup.

**Targeted upstream:** GO SMS Pro *(app version not pinned yet)* (`export_tool` on every output row; `export_tool_version` empty).

## What this is for

GO SMS Pro can save texts onto the phone as a backup folder. That folder usually contains:

- XML files named like `gosms_sys….xml` — ordinary SMS text messages
- files ending in `.pdu` — MMS messages (often with pictures or other media packed inside)

MMS `.pdu` files are decoded with a WAP-209 / WSP-inspired structured parser first: From/To/Cc/Date headers, Content-Location named parts (`text.txt`, images, …), SMIL `src` binding when present, then multipart parts on full PDUs. Older text-marker / magic-byte heuristics run only when a field is still empty. The decode path is modeled on the concepts in [python-messaging](https://github.com/pmarti/python-messaging) and the public WAP specs (reference only; not vendored).

There is no official public description of this backup format. For a detailed walkthrough of how each message becomes a spreadsheet row, see [docs/XML_CSV_MAPPING.md](docs/XML_CSV_MAPPING.md).

## What you get

- One CSV file per conversation (a CSV is a plain table you can open in Excel, Numbers, or Google Sheets)
- An `attachments/` folder next to those files for media pulled out of MMS backups
- Each row is one message: who it was with, when it was sent or received, the text, and whether media was attached

Example output from a small test backup: [`sample-output/`](sample-output/).

## What you need

1. The GO SMS Pro backup folder on disk
2. **Your phone number** — the number that owned the messages on that phone (required; there is no demo default)
3. **Contacts** (recommended) — `--contacts` (vault-shaped CSV) or `--vcf` so blank display names can be filled from phone numbers; without either, a warning is printed and names are left unresolved

For ordinary SMS in the XML backup, sent vs received comes from the backup’s own type field. Your number is still required so MMS (`.pdu`) direction and chat grouping are correct. For example, if your number is `+1 555 555 0100`, pass that (or the same digits without spaces) as `--owner-phone`.

## How to run

From the [message-exporters](../..) repository root:

```bash
cargo run --release -p go-sms-pro-out -- \
  --input /path/to/gosms_export \
  --output ./staging/go-sms-pro \
  --owner-phone +15555550100 \
  --contacts /path/to/contacts.csv
```

Replace the paths and phone number with your own. `--input` is the backup folder. `--output` is where the CSV files and `attachments/` folder are written. Use `--vcf` instead of `--contacts` if you have a VCF.

Add `--anonymize` (optional `--anonymize-seed <64-hex>`) to rewrite names, numbers, text, and attachments for sharing structure without PII. See [`message-anonymize`](../message-anonymize).

Optional `--start-date` / `--end-date` (`YYYY-MM-DD`) keep messages in `[start, end)` using host local midnight (end exclusive).

Attachment media (after export; needs `ffmpeg` / `ffprobe` for convert/compress):

- `--media-mode disabled` — do not write attachment files
- `--media-mode clone` (default) — leave files as extracted
- `--media-mode convert` — standardize to `.jpg` / `.mp4` / `.mp3`
- `--media-mode compress` — re-encode; options `--media-max-resolution 720p|1080p|4k`, `--media-max-fps`, `--media-min-size`, `--media-skip-efficient true|false`

See [`message-media`](../message-media).

## License

MIT.
