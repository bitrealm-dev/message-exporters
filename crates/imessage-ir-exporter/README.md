# imessage-ir-exporter

Export Apple Messages (`chat.db` / iOS backup) to **per-conversation CSV, EML, MBOX, JSON, or JSONL** via [`imessage-database`](https://crates.io/crates/imessage-database) and [`message-ir`](../message-ir/).

Pipeline: `chat.db` → `MailMessage` → `ConversationDocument` → `message_ir::write_format`.

## CLI

```bash
cargo run -p imessage-ir-exporter -- \
  --output ./out \
  --format json
```

Formats: `csv` (default), `eml`, `mbox`, `json`, `jsonl` (schema v3 IR).

## Library

```rust
use imessage_ir_exporter::run;
use message_exporters_core::ExporterConfig;

let result = run(&config)?;
```

Requires `SourceConfig::Apple`.

## GUI

**iPhone backup** dispatches here for all output formats. Convert / compress / obfuscate remain CSV post-steps.

## License

GPL-3.0-or-later (same as `imessage-database` / `crabapple`).

## See also

- [MESSAGE_IR.md](../../docs/MESSAGE_IR.md)
- [MAIL_ARCHIVE.md](../../docs/MAIL_ARCHIVE.md)
