# imessage-mail-exporter

Export Apple Messages to **per-conversation `.eml` mail archives** using [`imessage-database`](https://crates.io/crates/imessage-database) and [`message-mail`](../message-mail/).

This crate does **not** depend on `imessage-exporter`. CSV / TXT / HTML remain on that GPL fork.

## Library

```rust
use imessage_mail_exporter::run;
use message_exporters_core::ExporterConfig;

let result = run(&config)?;
```

Requires `SourceConfig::Apple` and `OutputFormat::Eml` or `OutputFormat::Mbox`.

## GUI

**iPhone backup** → Output format **EML** / **MBOX** dispatches here; **CSV** still uses `imessage-exporter`.

- **EML** — one folder per conversation of `.eml` files (canonical mail archive)
- **MBOX** — one `.mbox` per conversation (mboxrd), for mailbox import

Honored options: start/end dates, conversation filter, contacts / AddressBook, attachment embed vs disabled, `--use-caller-id` on outgoing From. Convert / compress / obfuscate stay CSV-only.

Content: plain-text bodies plus `X-ME-*` headers for replies, tapbacks (per-message EML + parent `X-ME-Tapbacks`), parts/edits, app balloons, send effects (`Sent with …`), announcements, SharePlay, shared location, deleted. Handwriting attaches SVG.

## License

GPL-3.0-or-later (same as `imessage-database` / `crabapple`).

## See also

- [MAIL_ARCHIVE.md](../../docs/MAIL_ARCHIVE.md)
- [imessage-exporter](../imessage-exporter/)
