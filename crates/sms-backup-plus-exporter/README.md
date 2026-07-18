# sms-backup-plus-exporter

This tool turns **[SMS Backup+](https://github.com/jberkel/sms-backup-plus)** email dumps (`.eml` files) into a clean set of one-message-per-file emails, or into [`message-json`](../message-json) **SMS** NDJSON for this vault.

Part of the [message-vault-rs](../..) Cargo workspace. Prefer the vault’s `ingest` command for a full export→import→dedupe pass; use the subcommands below when cleaning a messy EML tree by hand.

## Two kinds of input files

An export usually mixes two shapes of `.eml`:

1. **Flat** — one text or picture message in one file.  
   Example: `2015-08-24_165449_SMS.eml` with headers like `X-smssync-address` and `Subject: SMS with Nathan`.

2. **Archive** — many texts packed into one file.  
   Example: `SMS archive with Alice (2011-2013).eml`. Inside the body, each text starts with a line like `2012-05-24 14:20:31 - Alice`.

A messy backup often has the same flat message copied under several folders, plus archive files that repeat some of those texts. See [docs/FORMAT.md](docs/FORMAT.md) for header details.

## Preferred: vault ingest

When the EML tree is already clean enough, from the repo root:

```bash
cargo run --release -- ingest sms-backup-plus --from /path/to/eml-tree
```

Or with the archive helper: `./scripts/ingest-staging.sh sms-backup-plus`.

## Typical workflow (manual cleanup)

1. Run `dedupe-eml` on the messy export → `./clean-eml` (one clean copy per text, by year).
2. Optional: open `junk/unresolved_names.txt`, fix contacts / name mapping, re-run.
3. Run `convert` on the clean tree → NDJSON + attachments.
4. Import that folder into the vault (or point `ingest --from` at the clean tree).

## `dedupe-eml` — messy tree → one clean copy per message

`dedupe-eml` walks every `.eml` under the input folder. It keeps one flat file when the same text appears many times, and puts winners into year folders.

### Output layout

```
clean-eml/
  2015/
    20150824_165449_recv_19546515240_3ef3bc01e72c.eml
  2016/
    …
  junk/
    2014/
      …          # parsed SMS, but the phone number is still unknown
    unresolved_names.txt
  unparseable/
    message-1-59044.eml   # looked like SMS Backup+, but parsing failed
  dedupe.log
```

Filename pattern:

```
{YYYY}/{YYYYMMDD_HHMMSS}_{sent|recv}_{chat}_{short_id}.eml
```

`dedupe.log` records DROP / REPLACE / COPY / GENERATED / UNPARSEABLE (and unique MAP / CONTACT lines). Per-message KEEP and OVERLAP lines are omitted so large runs stay fast; overlap counts still appear in the summary.

### How two files count as the same text

Matching uses chat, time, direction, and body text (and hashes of any attached files when comparing two flats).

The `X-smssync-id` header is not trusted on its own — two different chats can use the same id (ex. `276`) for different people. When that header is present, the match key still includes chat, time, direction, and text.

Archive body times only go down to whole seconds (`2012-05-24 14:20:31`). Flat files often have millisecond dates. Archive lines are matched to flats using whole-second time and text (attachment hashes are ignored for that check), so a flat at `…488` ms still covers an archive line at the same second.

### Texts that exist only in an archive

Example: `SMS archive with Alice.eml` lists `2012-05-24 14:20:31 - Alice` / `See you Thursday`, but there is no matching flat like `20120524_142031_recv_….eml`.

For those cases, `dedupe-eml` generates a new flat `.eml` (logged as `GENERATED`). That file is not a copy of an original backup email. It uses a made-up `X-smssync-id` and a short set of headers.

### Contacts and name mapping

Some archive (and a few flat) messages only know a person’s name, not their phone. Those can be filled in with CSVs under the **repo root** `config/`:

| File | Columns | Role |
| --- | --- | --- |
| Repo `config/contacts.csv` | `phones,first_name,last_name,…` | Name → phone (same file the vault imports; `exclude=true` rows are skipped) |
| Repo `config/name-mapping.csv` | `correct_name,incorrect_name` | Fix messy subject names, then look up contacts |

Example: subject says `Casey Typo` → mapping rewrites to `Casey Proper` → contacts CSV supplies `+15555550888`.

Unresolved names stay under `junk/` and are listed in `junk/unresolved_names.txt`. The CLI reports **unique** names mapped and contacts resolved (not one count per message).

Defaults (lookup order when flags are omitted):

- Repo `config/contacts.csv` (or `../../config/contacts.csv` if the working directory is this crate)
- Repo `config/name-mapping.csv`, then this crate’s `config/name-mapping.csv` as a fallback
- Copy examples: [`config/name-mapping.csv.example`](../../config/name-mapping.csv.example) → `config/name-mapping.csv`, and [`config/contacts.csv.example`](../../config/contacts.csv.example) → `config/contacts.csv`

### CLI-only owner defaults (`owner.toml`)

Vault ingest uses `[owner]` from root `config/config.toml`. The exporter CLI does **not** read that file. For standalone `dedupe-eml` / `convert` runs, copy [`config/owner.example.toml`](config/owner.example.toml) → `config/owner.toml` under this crate (or `config/owner.toml` at the repo root) for default `--owner-phone` / `--owner-email` / `--input` (`source_dirs`). That file is CLI-only and gitignored under this crate.

CLI flags still override any defaults when provided.

```bash
cargo run --release -p sms-backup-plus-exporter -- -v dedupe-eml \
  --input /path/to/messy/exports \
  --output ./clean-eml \
  --name-mapping config/name-mapping.csv
```

Repeat `--input` to merge several trees (path-deduped before processing):

```bash
cargo run --release -p sms-backup-plus-exporter -- convert \
  --input /path/to/galaxy-export \
  --input /path/to/everything-archive \
  --output ./staging/sms-backup-plus-eml
```

Global flags (before or after the subcommand):

- `-v` / `--verbose` — progress on stderr every 5000 items  
- `--no-summary` — skip the end-of-run stats on stdout  

## `convert` — clean EML → NDJSON

`convert` reads a tree of flat `.eml` files (ideally the output of `dedupe-eml`) and writes the JSON format message-vault-rs expects (NDJSON), plus an `attachments/` folder for pictures and other media.

```bash
cargo run --release -p sms-backup-plus-exporter -- convert \
  --input ./clean-eml \
  --output ./staging/sms-backup-plus-eml
```

## Import into the vault

```bash
cargo run --release -- import --source sms-backup-plus --mode append
cargo run --release -- dedupe-cross-source
```

## Related tools

The older Python `sms_eml_master` pipeline in the personal `message-vault` repo turns the same kind of dump into per-contact conversation JSON for Obsidian. This crate focuses on cleaning EML files and writing NDJSON for this vault.
