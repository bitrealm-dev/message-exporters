# NAME

message-reexporter - convert an existing Message Exporters output directory to another packaging format

# SYNOPSIS

```text
message-reexporter --input <DIR> --output <DIR>
    [--format json|jsonl|csv|eml|mbox|xml]
    [--media-mode disabled|clone|convert|compress]
    [--media-max-resolution 720p|1080p|4k] [--media-max-fps <N>]
    [--media-min-size <SIZE>] [--media-skip-efficient true|false]
    [--obfuscate] [--obfuscate-seed <8-hex>]
```

# DESCRIPTION

Auto-detects a single input format among `csv`, `eml`, `mbox`, `json`, `jsonl`, and `xml` (`smses.xml`) in `--input`, loads conversations into the common message, then writes `--format` (default `json`) via the shared packaging pipeline (media modes + obfuscate included).

`--output` must differ from `--input`. The desktop GUI **Re-export** tab uses the same library path.

# OPTIONS

**--input** *DIR*
: Prior export directory (exactly one format class must be detected).

**--output** *DIR*
: Destination directory (must differ from input).

**--format** *json|jsonl|csv|eml|mbox|xml*
: Output packaging (`json` default).

**--media-mode** *MODE*
: `disabled`, `clone` (default), `convert`, or `compress`.

**--media-max-resolution**, **--media-max-fps**, **--media-min-size**, **--media-skip-efficient**
: Compress-only knobs (defaults `1080p`, `30`, `20M`, `true`).

**--obfuscate**, **--obfuscate-seed** *8-hex*
: Post-export obfuscation; seed must be exactly eight hex digits.

# EXAMPLES

```bash
cargo run -p message-reexporter -- \
  --input /path/to/prior-export \
  --output /path/to/new-export \
  --format eml
```
