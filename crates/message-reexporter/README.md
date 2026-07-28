# message-reexporter

Convert an existing **Message Exporters** output directory to another packaging format.

Auto-detects a single input format among `csv`, `eml`, `mbox`, `json`, `jsonl`, and `xml` (`smses.xml`), loads conversations into the common message, then writes via `FormatSink` (media modes + obfuscate included). Default `--format` is `json`.

```bash
cargo run -p message-reexporter -- \
  --input /path/to/prior-export \
  --output /path/to/new-export \
  --format eml
```

GUI: top tab **Re-export** (next to Message).
