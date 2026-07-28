# Experimental backups

Use these Message-tab sources only when they are the only backup available. Formats are reverse-engineered or incomplete compared with the supported paths (iPhone, SMS Backup & Restore, WhatsApp).

## Available experimental sources

| Backup type | Typical input |
|-------------|----------------|
| **GO SMS Pro** | GO SMS Pro XML + PDU folder |
| **iMazing** | iMazing Messages CSV export tree |
| **OpenExtract** | OpenExtract export + contacts VCF when available |
| **SMS Backup+** | SMS Backup+ mail / archive layout |

Each source still writes the shared output formats (CSV, EML, MBOX, JSON, JSON Lines, XML) through the same packaging pipeline. Expect missing fields, weaker attachment handling, or name/phone gaps depending on the source.

## Steps (desktop app)

1. Open **Message**.
2. In **Backup type**, select an experimental source (labeled **experimental** in the list, below the supported sources).
3. Select **Output format**.
4. Fill the required fields for that source (input path, owner phones, emails, timezone, and so on).
5. Set **Output directory**.
6. Choose **Attachments** and optional date range or **Obfuscate**.
7. Select **Run exporter**.
8. Review the **Log** carefully for skipped messages and unresolved contacts.

## When to prefer a supported path

1. Prefer **SMS Backup & Restore** when a SyncTech XML backup can be produced instead of GO SMS Pro or SMS Backup+.
2. Prefer **iPhone backup** over iMazing CSV when a `chat.db` or iOS backup is available.
3. Prefer **WhatsApp** native extract over third-party WhatsApp CSV when crypt/key material is available.

## Related

- [Use the desktop app](desktop-app.md)
- [Choose an output format](formats.md)
- CLI details under [CLI reference](../reference/go-sms-pro.md)
