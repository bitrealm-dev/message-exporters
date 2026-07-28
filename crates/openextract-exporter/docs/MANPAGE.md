# NAME

openextract-exporter - convert OpenExtract conversation CSV (+ VCF) via common message to JSON/CSV/EML/MBOX/JSONL/XML

# SYNOPSIS

```text
openextract-exporter --input <PATH> --output <DIR>
    [--format json|jsonl|csv|eml|mbox|xml]
    [--vcf <PATH> | --contacts <PATH>]
    [--start-date YYYY-MM-DD] [--end-date YYYY-MM-DD]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

# DESCRIPTION

Reads OpenExtract conversation CSV (`all_conversations.csv` or `conversation_*.csv`, file or directory), builds a common message per conversation, and projects JSON (default) or another `--format`. Contacts (`--vcf` or `--contacts`) are recommended so names and phones resolve; without them export still runs with a warning.

This converter does not extract binary media attachments.

# OPTIONS

**--input** *PATH*
: Conversation CSV file or directory of OpenExtract CSVs.

**--output** *DIR*
: Destination for packaging output.

**--format** *json|jsonl|csv|eml|mbox|xml*
: Output packaging from the common message (`json` default).

**--vcf** *PATH*
: Contacts VCF from the OpenExtract export.

**--contacts** *PATH*
: Contacts file instead of `--vcf` (VCF or iMazing Contacts CSV). At most one of `--contacts` / `--vcf`.

**--start-date** *YYYY-MM-DD*
: Include messages on or after this local date (inclusive).

**--end-date** *YYYY-MM-DD*
: Include messages before this local date (exclusive).

**--obfuscate**
: Rewrite names, numbers, and text with stable fakes after export.

**--obfuscate-seed** *8-hex*
: Exactly eight hexadecimal characters; implies `--obfuscate`.

# EXIT STATUS

Non-zero on invalid paths, parse/convert failure, or bad date/seed arguments.

# FILES

**Input**
: OpenExtract conversation CSV(s); optional contacts VCF/CSV.

**Output**
: One `*.csv` per conversation. Name-only chat ids may remain unresolved for vault import.

# ENVIRONMENT

None required beyond a normal process environment.

# EXAMPLES

```bash
openextract-exporter \
  --input /path/to/openextract_csv_dir \
  --output ./staging/openextract \
  --vcf /path/to/contacts.vcf
```

# NOTES

Experimental in the GUI. Thin source format: no groups, no media extraction; contacts strongly recommended. See the crate README for sample output.

# SEE ALSO

[README.md](../README.md),
[message-contacts](../../message-contacts),
[message-obfuscate](../../message-obfuscate)
