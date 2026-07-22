# GO SMS Pro → CSV

The `go-sms-pro-out` converter transforms a **GO SMS Pro** (GOMO / Jiubang) Android backup into `.csv` files, one file per conversation with any attachments found in that conversation.

## What this is for

GO SMS Pro can save texts onto a phone in a backup folder. That folder usually contains:

- XML files named like `gosms_sys….xml` — ordinary SMS text messages
- files ending in `.pdu` — MMS messages (often with pictures or other media packed inside)

MMS `.pdu` files are binary files that are not human readable. GO SMS Pro appears to store each MMS as a packed binary blob: phone numbers, the message text, and any photos or other media are encoded into one file. Those files often look like pieces of a real phone MMS, but are not always a complete message.

Historically, phones encoded MMS messages according to the MMS Encapsulation Protocol (formerly known as WAP-209). The protocol evolved over the years and was later published as the more [modern Open Mobile Alliance version](https://www.openmobilealliance.org/release/MMS/V1_3-20110913-A/OMA-TS-MMS_ENC-V1_3-20110913-A.pdf). It describes how an MMS is laid out—contacts, text, and media in one binary message. [WSP](https://www.openmobilealliance.org/tech/affiliates/wap/wap-230-wsp-20010705-a.pdf) (Wireless Session Protocol, also known as WAP-230) is the companion labeling system for the pieces inside that message—for example marking something as plain text or a JPEG and giving it a filename.

The converter first tries a structured parser built around those rules. It looks for the usual message structure and pulls out contacts, text, and media when it can find them. If something is still missing after that pass, it falls back to simpler searches—for example looking for known text markers or the telltale start of a JPEG. The decode approach is guided by those public specs and ideas from [python-messaging](https://github.com/pmarti/python-messaging) (reference only; nothing from that project is bundled into this tool).

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
