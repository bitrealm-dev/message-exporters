# GO SMS Pro → CSV

The `go-sms-pro-out` converter transforms a **GO SMS Pro** (GOMO / Jiubang) Android backup into `.csv` files, one file per conversation with any attachments found in that conversation.

## What this is for

GO SMS Pro can save texts (both SMS and MMS) onto a phone in a backup folder. That folder usually contains:

- XML files named like `gosms_sys….xml` — ordinary SMS text messages
- files ending in `.pdu` — MMS messages (often with pictures or other media packed inside)

MMS `.pdu` files are binary files that are not human readable. GO SMS Pro appears to store each MMS as a packed binary blob: phone numbers, the message text, and any photos or other media are encoded into one file. Those files often look like pieces of a real phone MMS, but are not always a complete message.

For years, phones packed MMS the same way using a public recipe called the MMS Encapsulation Protocol (first published as [WAP-209](docs/wap-209-mmsencapsulation-20020105-a.pdf)). That recipe says how to put contacts, text, and media into one binary message. A later revision is the [Open Mobile Alliance MMS Encapsulation specification](docs/OMA-TS-MMS_ENC-V1_3-20110913-A.pdf).

A second standard, [WSP](docs/wap-230-wsp-20010705-a.pdf) (Wireless Session Protocol, also called WAP-230), labels the pieces inside that message. For example, it can mark one piece as plain text, another as a JPEG, and attach a filename.

GO SMS Pro has not publicly described this backup format, and the saved files do not always follow those standards closely. The `go-sms-pro-out` converter still tries to read each `.pdu` using that protocol layout. It pulls out contacts, text, and media when it can find them. If something is still missing after that pass, the converter falls back to simpler searches—for example looking for known text markers or the telltale start of a JPEG.

For a detailed walkthrough of how each message becomes a spreadsheet row, see [docs/XML_CSV_MAPPING.md](docs/XML_CSV_MAPPING.md).

## What you get

- One CSV file per conversation, easily viewed in Excel, Numbers, or Google Sheets
- An `attachments/` folder next to those files for media pulled out of MMS backups
- Each row is one message: who it was with, when it was sent or received, the text, and whether media was attached

Example output from a small test backup: [`sample-output/`](sample-output/).

## What you need

1. The GO SMS Pro backup folder on disk
2. **Your phone number** (required) — pass as `--owner-phone` (for example `+15555550100`). Ordinary SMS in the XML backup already records sent vs received, but this number is still needed so MMS (`.pdu`) direction and chat grouping are correct.
3. **Contacts** (recommended) — a contacts file so blank display names can be filled from phone numbers. Use the same formats as **Contacts → Check** in the desktop app: a VCF, or an iMazing Contacts CSV (First Name, Last Name, and a phone column such as Mobile Phone). Pass with `--contacts` or `--vcf`. Numbers work best in E.164 form (for example `+15555550100`). Without either file, a warning is printed and names stay unresolved.

## How to run

From the [message-exporters](../..) repository root:

```bash
cargo run --release -p go-sms-pro-out -- \
  --input /path/to/gosms_export \
  --output ./staging/go-sms-pro \
  --owner-phone +15555550100 \
  --contacts /path/to/contacts.csv
```

Replace the paths and phone number with your own. `--input` is the backup folder. `--output` is where the CSV files and `attachments/` folder are written. `--contacts` and `--vcf` both accept a validated contacts file; use whichever flag matches how you export contacts.

Add `--anonymize` (optional `--anonymize-seed <64-hex>`) to rewrite names, numbers, text, and attachments for sharing structure without PII. See [`message-anonymize`](../message-anonymize).

Optional `--start-date` / `--end-date` (`YYYY-MM-DD`) keep messages in `[start, end)` using host local midnight (end exclusive).

Attachment media (after export; needs `ffmpeg` / `ffprobe` for convert/compress):

- `--media-mode disabled` — do not write attachment files
- `--media-mode clone` (default) — leave files as extracted
- `--media-mode convert` — standardize to `.jpg` / `.mp4` / `.mp3`
- `--media-mode compress` — re-encode; options `--media-max-resolution 720p|1080p|4k`, `--media-max-fps`, `--media-min-size`, `--media-skip-efficient true|false`

See [`message-media`](../message-media).

## Thanks

[python-messaging](https://github.com/pmarti/python-messaging) — reference implementation for MMS decoding.

## License

MIT.
