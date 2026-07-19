# Message Exporters GUI

Living design notes for the cross-platform desktop GUI that drives the existing exporter binaries.

**Framework:** [Iced](https://iced.rs/) 0.14, implemented in
[`crates/message-exporters-gui`](../crates/message-exporters-gui).

## Goals

- One app for Linux, macOS, and Windows with a native look and feel.
- Spawn the existing CLI exporters; do not reimplement conversion logic in the UI.
- Show only the controls that apply to the selected backup source; validate before run.
- Stream converter stdout/stderr in the UI; support cancel.
- Prefer plain-language labels and product site links over CLI jargon.

## Current implementation

- Pure Rust Iced desktop app for Linux, macOS, and Windows.
- Typed forms and CLI argument builders for every backup source converter.
- Native file/folder dialogs through `rfd`.
- Exporter discovery beside the GUI executable, in `MESSAGE_EXPORTERS_BIN`, then on `PATH`.
- Live tagged stdout/stderr log and process cancellation.
- Exporter-specific validation before launch.
- Backup-source titles link to the upstream product site.

Build all sibling executables, then run:

```bash
cargo build --workspace
cargo run -p message-exporters-gui
```

## Non-goals

- Packaging / installers.
- Changing exporter CLIs (except noting gaps for later).
- Exposing `imazing-anonymize` in the GUI (CLI-only for now).

## Shared controls

Every converter screen exposes:

| Control | Widget | CLI mapping | Notes |
|---------|--------|-------------|-------|
| Backup source | labeled selector | which binary | Sorted alphabetically by display name |
| Product title | hyperlink | — | Opens the upstream product site |
| Input | path picker (file and/or folder) | `--input` / `-p` / etc. | Single path only |
| Output | folder picker | `--output` / `-o` | Required; defaults to Documents/`message-exporters`/<source> |
| Contacts | path picker | `--contacts` / `--vcf` / `-n` | Format depends on exporter; optional with warning |
| Anonymize | checkbox | `--anonymize` | When enabled, show seed field |
| Anonymize seed | text (64-hex) | `--anonymize-seed` | Optional; blank = generate at run time |
| Run / Cancel | actions | spawn process | Stream logs; kill on cancel |

## Show / hide by backup source

| Section | GO SMS Pro | Backup & Restore | SMS Backup+ | OpenExtract | iMazing | iPhone backup |
|---------|:----------:|:----------------:|:-----------:|:-----------:|:-------:|:-------------:|
| Input / Output | yes | yes | yes | yes | yes | yes |
| DB path / Platform | — | — | — | — | — | primary |
| Anonymize | yes | yes | yes | yes | gap\* | yes (CSV) |
| Your phone number(s) | required | required | required\*\* | — | — | — |
| Your email address(es) | — | — | required\*\* | — | — | — |
| Contacts vault CSV / VCF | yes | yes | yes | yes | — | — |
| Contacts iMazing CSV | — | — | — | — | yes | — |
| Contacts Apple AddressBook | — | — | — | — | — | advanced |
| Timezone | — | — | — | — | yes | — |
| Name mapping | — | — | advanced | — | — | — |
| Verbose logging | — | — | always on | — | — | — |
| Copy method / advanced | — | — | — | — | — | yes |

\* `imazing-out` currently has **no** `--anonymize` flag — hide until parity is added.  
\*\* Required unless filled from Plus `config/owner.toml` (source-relative today); GUI collects fields explicitly.

## Per-exporter options

### GO SMS Pro — `go-sms-pro-out`

Product: [GO SMS Pro](https://play.google.com/store/apps/details?id=com.jb.gosms)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | folder (backup root with XML + PDU) | yes | `--input` |
| Output | folder | yes | `--output` |
| Your phone numbers | multi-value text | yes | `--owner-phone` (repeat) |
| Contacts CSV | file | no† | `--contacts` |
| Contacts VCF | file | no† | `--vcf` |
| Anonymize / seed | bool / text | no | `--anonymize`, `--anonymize-seed` |

† At most one of `--contacts` / `--vcf`.

### SMS Backup & Restore — `sms-backup-restore-out`

Product: [SMS Backup & Restore](https://www.synctech.com.au/sms-backup-restore/)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | XML file or folder of XML | yes | `--input` |
| Output | folder | yes | `--output` |
| Your phone numbers | multi-value text | yes | `--owner-phone` |
| Contacts CSV / VCF | file | no† | `--contacts` / `--vcf` |
| Anonymize / seed | bool / text | no | `--anonymize`, `--anonymize-seed` |

Encrypted ZIP backups must be unlocked/extracted before selecting input.

### SMS Backup+ — `sms-backup-plus-out convert`

Product: [SMS Backup+](https://github.com/jberkel/sms-backup-plus)

GUI always runs the `convert` subcommand and always passes `--verbose`.

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | one EML file or folder | yes | `--input` |
| Output | folder | yes | `--output` |
| Your phone numbers | multi-value text | yes\* | `--owner-phone` |
| Your email addresses | multi-value text | yes\* | `--owner-email` |
| Contacts CSV / VCF | file | no† | `--contacts` / `--vcf` |
| Name mapping CSV | file | no | `--name-mapping` (advanced) |
| Verbose | — | always | `--verbose` |
| Anonymize / seed | bool / text | no | `--anonymize`, `--anonymize-seed` |

\* Or from crate-relative `config/owner.toml` — GUI does not rely on that; collect explicitly.

### OpenExtract — `openextract-out`

Product: [OpenExtract](https://www.openextract.app/)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | CSV file or folder | yes | `--input` |
| Output | folder | yes | `--output` |
| Contacts VCF / vault CSV | file | no† | `--vcf` / `--contacts` |
| Anonymize / seed | bool / text | no | `--anonymize`, `--anonymize-seed` |

### iMazing — `imazing-out`

Product: [iMazing](https://imazing.com/)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | Messages/WhatsApp CSV, chat folder, `Messages/`, `WhatsApp/`, or device export root | yes | `--input` |
| Output | folder | yes | `--output` |
| Contacts | iMazing Contacts CSV only | no | `--contacts` |
| Timezone | IANA text | no | `--timezone` (default: host local) |
| Anonymize | — | — | **not implemented** (gap) |

WhatsApp chats write as separate `…__whatsapp.csv` files. See [`crates/imazing-out/docs/DESIGN.md`](../crates/imazing-out/docs/DESIGN.md).

### iPhone backup — `imessage-exporter`

Form link label: **iPhone backup - imessage-exporter** → [imessage-exporter](https://github.com/ReagentX/imessage-exporter). Dropdown stays **iPhone backup**.

GUI defaults: `-f csv`, `-c clone`, always `--use-caller-id`.

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Database / iOS backup path | file/folder | no | `-p` / `--db-path` |
| Backup password | password | no | `-x` / `--cleartext-password` |
| Platform | macOS / iOS / auto | no | `-a` / `--platform` |
| Output / export path | folder | yes | `-o` / `--export-path` |
| Copy method | enum | no | `-c` (`clone` recommended) |
| Anonymize / seed | bool / text | no | `--anonymize`, `--anonymize-seed` |
| Attachment root | folder | no | `-r` / `--attachment-root` (advanced) |
| Start / end date | date | no | `-s` / `-e` (advanced) |
| Conversation filter | text | no | `-t` (advanced) |
| Contacts (AddressBook DB) | file | no | `-n` / `--contacts-path` (advanced) |

Not exposed in the GUI: `--custom-name`, `--ignore-disk-warning` (CLI-only). Caller ID is always on.

Advanced panel uses a chevron toggle (**Show advanced options**), not a checkbox.

## Validation rules

1. **Contacts mutual exclusion:** for Android/OpenExtract, allow at most one of vault CSV vs VCF.
2. **Contacts format:** label and file filters must match the exporter (vault/VCF vs iMazing CSV vs Apple AddressBook).
3. **Phone numbers:** required for GO SMS Pro and SMS Backup & Restore before Run; Plus also requires email address(es).
4. **Path existence:** input must exist; output folder may be created on run.
5. **Anonymize seed:** if provided, must be 64 hex characters; empty means generate.
6. **Timezone (iMazing):** if set, must be a valid IANA name (or defer to converter error).
7. **iPhone backup:** output directory is required; always passes `--use-caller-id`; anonymize only applies to CSV.
8. **SMS Backup+:** exactly one input path; GUI always prefixes `convert` and always passes `--verbose`.
9. **Warn (non-blocking):** missing contacts → same warning language as CLIs (“phones will not be resolved to names”).

## Form flow

```text
Pick backup source
  → linked product title
  → common: Input, Output, Contacts (typed), Anonymize
  → conditional: phone / email | Timezone | advanced options
  → Run / Cancel
  → always-on run log
```

## Known gaps

| Gap | Detail | Suggested fix |
|-----|--------|---------------|
| iMazing anonymize | Root README says any converter; `imazing-out` has no flags | Add `--anonymize` / `--anonymize-seed` like peers |
| Plus `owner.toml` | Resolved via `CARGO_MANIFEST_DIR`, not user cwd | GUI collects phone/email/input explicitly |
| iMessage flag style | Short flags (`-f`, `-c`, `-o`) vs long `--input` family | GUI abstracts; map to correct argv |
| iMazing attachments | Filename-only; no media copy | Document in UI; optional future media join |
| Encrypted backup password | Visible in process list if passed as argv | Prefer env/stdin if CLI grows support; warn in UI |

## Next steps

- Add iMazing anonymize CLI parity before showing the checkbox for that exporter.
- Add application icons and native installers/packages.
- Persist recently used paths and non-secret preferences.
- Add platform CI builds and GUI smoke tests.
