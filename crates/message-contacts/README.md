# message-contacts

Shared **name ↔ phone** resolution for Android CSV exporters.

Load a vault-shaped contacts CSV (`phones,first_name,last_name[,exclude,…]`) or a VCF, then:

- **name → phone** — fill missing chat peer numbers (SMS Backup+)
- **phone → name** — fill blank / `unknown` display names (GO SMS Pro, SMS Backup & Restore, Plus)

Name resolution belongs in **message-exporters** (backup → CSV), not in vault `csv-ingest`. CSV is the human checkpoint: inspect and correct before convert.

## CLI helper

```rust
use message_contacts::resolve_contacts_cli;

let (book, path) = resolve_contacts_cli(contacts_opt, vcf_opt)?;
// At most one of --contacts or --vcf. Neither → empty book + stderr warning.
```

## License

MIT.
