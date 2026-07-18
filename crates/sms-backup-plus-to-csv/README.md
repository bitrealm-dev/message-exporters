# sms-backup-plus-to-csv

Convert [SMS Backup+](https://github.com/jberkel/sms-backup-plus) `.eml` trees into per-conversation **CSV**. Reuses iMessage field names where the concept exists; unused iMessage-only columns are omitted. Backup+-only fields are appended. A universal CSV shared by all exporters is a non-goal.

EML format notes: [`docs/FORMAT.md`](docs/FORMAT.md). EML → CSV field mapping: [`docs/EML_CSV_MAPPING.md`](docs/EML_CSV_MAPPING.md). Example output from the test fixtures: [`samples/`](samples/).

## Standalone usage

From the repo root:

```bash
cargo run --release -p sms-backup-plus-to-csv -- convert \
  --input /path/to/eml_export \
  --output ./staging/sms-backup-plus-csv \
  --owner-phone +15555550100 \
  --owner-email you@example.com
```

Repeat `--owner-phone` / `--owner-email` for multiple identities (required via CLI or `config/owner.toml` — no silent demo defaults). Optional: `--contacts`, `--name-mapping`.

Progress logging: pass `-v` / `--verbose` for owner/contacts paths, scan progress (every 5000 EMLs), parse summary, write progress, and end-of-run dedupe counts (stderr).

Output:

- one `.csv` file per conversation
- `attachments/` for MIME media
- online fingerprint dedupe while scanning (earlier timestamp wins; see mapping doc)
- Backup+-only columns: `export_source` (`sms-backup-plus`), `source_kind`, `smssync_id`, `date_ms`, `contact_name`, `android_type`, `eml_path`

## License

MIT.
