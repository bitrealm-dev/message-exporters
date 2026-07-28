# Re-export between formats

Convert a prior Message Exporters output folder to another packaging format without re-reading the phone backup. Use this to escape format lock-in (for example CSV → EML, or JSON → XML).

## Before starting

1. Locate an existing export directory produced by Message Exporters (one clear format among CSV, EML, MBOX, JSON, JSONL, or `smses.xml`).
2. Create a **different** empty output directory. Input and output must not be the same path.
3. Prefer folders that still include `attachments/` when media should carry over.

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
| IR `.json` files (`schema_version` 3) | JSON |
| IR `.jsonl` files | JSONL |
| Unified IR `.csv` files | CSV |
| `.mbox` files | MBOX |
| Subfolders containing `.eml` | EML |

`attachments/` and `*.meta.json` are ignored for detection.

## Result

The output directory holds the new packaging. The log reports the detected input format and how many conversations were written.

## If it fails

- **Mixed formats** — remove extra format classes from the input folder, or re-export from a clean single-format export.
- **Unsupported input** — the folder is not a Message Exporters IR layout.
- **Same path** — choose a distinct output directory.
- **XML owner issues** — outgoing MMS owner is inferred when possible; SMS-only backups usually still load.

XML re-export can drop Apple-only fields. Prefer JSON or CSV as the intermediate when preserving iMessage detail matters. See [Choose an output format](formats.md).
