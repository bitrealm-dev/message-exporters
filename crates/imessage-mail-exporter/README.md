# imessage-mail-exporter

Export Apple Messages to **per-conversation `.eml` mail archives** using [`imessage-database`](https://crates.io/crates/imessage-database) and [`message-mail`](../message-mail/).

This crate does **not** depend on `imessage-exporter`. CSV / TXT / HTML remain on that GPL fork.

## Library

```rust
use imessage_mail_exporter::run;
use message_exporters_core::ExporterConfig;

let result = run(&config)?;
```

Requires `SourceConfig::Apple` and `OutputFormat::Eml`.

## GUI

**iPhone backup** → Output format **EML** dispatches here; **CSV** still uses `imessage-exporter`.

## License

GPL-3.0-or-later (same as `imessage-database` / `crabapple`).

## See also

- [MAIL_ARCHIVE.md](../../docs/MAIL_ARCHIVE.md)
- [imessage-exporter](../imessage-exporter/)
