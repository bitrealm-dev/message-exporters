# Re-export between formats

Convert a prior Message Exporters output folder to another packaging format without re-reading the phone backup. The tool reads conversations back into the [common message](common-message.md), then writes the format you pick.

Use Re-export only when you already have an export folder (for example CSV → EML, or JSON → XML). A first export from a phone backup can write the target format in one Run.

## Before starting

1. Locate an existing export directory produced by Message Exporters (one clear format among CSV, EML, MBOX, JSON, JSONL, or `smses.xml`).
2. Create a **different** empty output directory. Input and output must not be the same path.
3. Prefer folders that still include media (sidecar `attachments/` for JSON / JSONL / CSV, or embedded media for EML / MBOX / XML).

## Steps (desktop app)

1. Open **Re-export**.
2. Set **Input directory** to the prior export folder.
3. Select **Output format**.
4. Set **Output directory**.
5. Choose **Attachments** (Copy keeps media when present).
6. Optional: enable **Obfuscate**.
7. Select **Run re-export**.
8. Review the **Log** for the detected input format and conversation count.

## Steps (CLI)

```bash
message-reexporter \
  --input /path/to/prior-export \
  --output /path/to/new-export \
  --format csv
```

Replace `csv` with `eml`, `mbox`, `json`, `jsonl`, or `xml` as needed. Media and obfuscate flags match the other exporters (`--media-mode`, `--obfuscate`, `--obfuscate-seed`).

## Auto-detect rules

The tool inspects the top level of the input folder and requires **exactly one** format class:

| Signal | Detected format |
|--------|-----------------|
| `smses.xml` | XML |
| Common-message `.json` files (`schema_version` 3) | JSON |
| Common-message `.jsonl` files | JSONL |
| Unified common-message `.csv` files | CSV |
| `.mbox` files | MBOX |
| Subfolders containing `.eml` | EML |

`attachments/` and leftover `*.meta.json` files (no longer written) are ignored for detection.

## Result

The output directory holds the new packaging. The log reports the detected input format and how many conversations were written.

## If it fails

- **Mixed formats** — remove extra format classes from the input folder, or re-export from a clean single-format export.
- **Unsupported input** — the folder is not a Message Exporters common-message layout.
- **Same path** — choose a distinct output directory.
- **XML owner issues** — outgoing MMS owner is inferred when possible; SMS-only backups usually still load.

XML re-export can drop Apple-only fields. Prefer JSON when preserving iMessage detail matters. See [Common message](common-message.md) and [Formats in detail](formats.md).
