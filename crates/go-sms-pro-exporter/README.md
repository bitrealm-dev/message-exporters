# GO SMS Pro → common message → packaging

The `go-sms-pro-exporter` crate transforms a **GO SMS Pro** (GOMO / Jiubang) Android backup into the [common message](../../docs/src/common-message.md), then packages JSON (default), CSV, EML, MBOX, JSONL, or XML—one conversation at a time, with attachments when present.

It is both a **library** (`go_sms_pro_exporter::run` / `convert_export`) used by the desktop GUI and a **standalone CLI** binary of the same name. CLI reference (man-page style): [`docs/MANPAGE.md`](docs/MANPAGE.md).

## What this is for

GO SMS Pro can save texts (both SMS and MMS) onto a phone in a backup folder. That folder usually contains:

- XML files named like `gosms_sys….xml` — ordinary SMS text messages
- files ending in `.pdu` — MMS messages (often with pictures or other media packed inside)

MMS `.pdu` files are binary files that are not human readable. GO SMS Pro appears to store each MMS as a packed binary blob: phone numbers, the message text, and any photos or other media are encoded into one file. Those files often look like pieces of a real phone MMS, but are not always a complete message.

For years, phones packed MMS the same way using a public recipe called the MMS Encapsulation Protocol (first published as [WAP-209](docs/wap-209-mmsencapsulation-20020105-a.pdf)). That recipe says how to put contacts, text, and media into one binary message. A later revision is the [Open Mobile Alliance MMS Encapsulation specification](docs/OMA-TS-MMS_ENC-V1_3-20110913-A.pdf).

A second standard, [WSP](docs/wap-230-wsp-20010705-a.pdf) (Wireless Session Protocol, also called WAP-230), labels the pieces inside that message. For example, it can mark one piece as plain text, another as a JPEG, and attach a filename.

GO SMS Pro has not publicly described this backup format, and the saved files do not always follow those standards closely. The `go-sms-pro-exporter` converter still tries to read each `.pdu` using that protocol layout. It pulls out contacts, text, and media when it can find them. If something is still missing after that pass, the converter falls back to simpler searches—for example looking for known text markers or the telltale start of a JPEG.

For a detailed walkthrough of how each message becomes a spreadsheet row, see [docs/XML_CSV_MAPPING.md](docs/XML_CSV_MAPPING.md). That doc also defines the CLI skip counters (`skipped invalid address`, `skipped empty pdu`, and others).

## What you get

- One CSV file per conversation, easily viewed in Excel, Numbers, or Google Sheets
- An `attachments/` folder next to those files for media pulled out of MMS backups
- Each row is one message: who it was with, when it was sent or received, the text, and whether media was attached
- Diagnostic CSVs when skips occur: `skipped_invalid_address.csv`, `skipped_empty_pdu.csv`, `skipped_no_party.csv`

Shared CSV columns: [`docs/src/csv-output.md`](../../docs/src/csv-output.md).

## What you need

1. The GO SMS Pro backup folder on disk
2. **Your phone number** (required) — pass as `--owner-phone` (for example `+15555550100`). Ordinary SMS in the XML backup already records sent vs received, but this number is still needed so MMS (`.pdu`) direction and chat grouping are correct.
3. **Contacts** (recommended) — a contacts file so blank display names can be filled from phone numbers. Use the same formats as **Contacts → Check** in the desktop app: a VCF, or an iMazing Contacts CSV (First Name, Last Name, and a phone column such as Mobile Phone). Pass with `--contacts` or `--vcf`. Numbers work best in E.164 form (for example `+15555550100`). Without either file, a warning is printed and names stay unresolved.

## How to run

From the [message-exporters](../..) repository root:

```bash
cargo run --release -p go-sms-pro-exporter -- \
  --input /path/to/gosms_export \
  --output ./staging/go-sms-pro \
  --owner-phone +15555550100 \
  --contacts /path/to/contacts.csv
```

Replace the paths and phone number with your own. Full CLI (dates, media modes, obfuscate): [`docs/MANPAGE.md`](docs/MANPAGE.md).

## Thanks

[python-messaging](https://github.com/pmarti/python-messaging) — reference implementation for MMS decoding.

## License

MIT.
