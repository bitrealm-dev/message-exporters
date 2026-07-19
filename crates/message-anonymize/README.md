# message-anonymize

Shared library and tools to rewrite exporter CSV output so it keeps message **structure** (chats, timestamps, directions, attachment counts) without exposing real names, phone numbers, message bodies, or media bytes.

Remaps are **stable** for a given secret seed (HMAC-SHA256) and **not reversible** from the CSV alone. No real→fake mapping file is written.

## Flags on converters

Every near-vault converter accepts:

- `--anonymize` — rewrite the output directory after convert
- `--anonymize-seed <64-hex>` — reproducible remaps (implies anonymize). If omitted, a random seed is printed once to stderr.

## iMazing CSV rewriter

iMazing vendor CSV is not converted here—only rewritten:

```bash
cargo run --release -p message-anonymize --bin imazing-anonymize -- \
  --input /path/to/imazing.csv \
  --output ./staging/imazing-anon
```

Optional: `--anonymize-seed <hex>`.

## What changes

| Field | Behavior |
|-------|----------|
| Phone numbers | Same digit count; leading `+` kept when present |
| Display names | Human first + last from a fixed word list |
| Message text | Same character length; digest-driven filler |
| Attachments | Shared placeholders: image → `placeholder.jpg`, video → `placeholder.mp4`, other → `placeholder.bin` |
