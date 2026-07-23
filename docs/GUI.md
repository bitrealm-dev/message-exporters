# Message Exporters GUI

Living design notes for the cross-platform desktop GUI that drives the existing exporter binaries.

**Framework:** [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) 0.31, implemented in
[`crates/message-exporters-gui`](../crates/message-exporters-gui).

## Goals

- One app for Linux, macOS, and Windows with a native look and feel.
- Spawn the existing CLI exporters; do not reimplement conversion logic in the UI.
- Show only the controls that apply to the selected backup source; validate before run.
- Stream converter stdout/stderr in the UI; support cancel.
- Prefer plain-language labels and product site links over CLI jargon.

## Current implementation

- Pure Rust egui/eframe desktop app for Linux, macOS, and Windows.
- Top tab panel: **Validate contacts** (default, first) | **Export**.
- Typed forms and CLI argument builders for every backup source converter (`exporters.rs`).
- Native file/folder dialogs through `rfd`.
- Exporter / tool discovery beside the GUI executable, in `MESSAGE_EXPORTERS_BIN`, then on `PATH`.
- Live tagged stdout/stderr log and process cancellation (mpsc poll in `update`).
- Exporter-specific validation before launch.
- Backup-source titles link to the upstream product site.
- **Global options** (Anonymize + Start/End date) above the per-source form (Export tab).

Build all sibling executables, then run:

```bash
cargo build --workspace
cargo run -p message-exporters-gui
```

## Non-goals

- Packaging / installers.

## Layout

1. Top tabs — **Validate contacts** | **Export**
2. **Validate contacts:** contacts file, USA numbers checkbox, Check / Update / Cancel
3. **Export:** backup source picker + global options + per-source form
4. Shared run log (bottom panel)

### Validate contacts

Spawns [`contacts-validate`](../crates/message-contacts) (same discovery rules as exporters).

- **Check** (`--check`): dry run — no files written; the run log shows the same UNCERTAIN / DUPLICATE / summary content as a validate log.
- **Update**: write `<stem>-update.<ext>` (or `<stem>-update-N` when re-updating) (+ `.log`; CSV also `.vcf`). Only unambiguous phones are rewritten; uncertain values stay as-is.
- **Cancel**: stop the running job.

## Shared / global controls

| Control | Widget | CLI mapping | Notes |
|---------|--------|-------------|-------|
| Backup source | labeled selector | which binary | Sorted alphabetically by display name |
| Anonymize | checkbox (global) | `--anonymize` | When enabled, show seed field |
| Seed | text (64-hex, global) | `--anonymize-seed` | Optional; blank = generate at run time |
| Start date | text (global) | `--start-date` | Optional `YYYY-MM-DD`, inclusive |
| End date | text (global) | `--end-date` | Optional `YYYY-MM-DD`, exclusive |
| Product title | hyperlink | — | Opens the upstream product/tool site |
| Input | path picker (file and/or folder) | `--input` / `-p` / etc. | Single path only |
| Output | folder picker | `--output` / `-o` | Required; defaults to Documents/`message-exporters`/<source> |
| Contacts | path picker | `--contacts` / `--vcf` / `-n` | Format depends on exporter; optional with warning |
| Run / Cancel | actions | spawn process | Stream logs; kill on cancel |

## Show / hide by backup source

| Section | GO SMS Pro | Backup & Restore | SMS Backup+ | OpenExtract | iMazing | iPhone backup |
|---------|:----------:|:----------------:|:-----------:|:-----------:|:-------:|:-------------:|
| Global anon + dates | yes | yes | yes | yes | yes | yes |
| Input / Output | yes | yes | yes | yes | yes | yes |
| DB path / Platform | — | — | — | — | — | primary |
| Your phone number(s) | required | required | required\* | — | — | — |
| Your email address(es) | — | — | required\* | — | — | — |
| Contacts VCF / iMazing CSV | yes | yes | yes | yes | — | — |
| Contacts iMazing CSV | — | — | — | — | yes | — |
| Contacts Apple AddressBook | — | — | — | — | — | advanced |
| Timezone | — | — | — | — | yes | — |
| Name mapping | — | — | advanced | — | — | — |
| Verbose logging | — | — | always on | — | — | — |
| Attachments (copy/convert/compress/do not copy) | yes | yes | yes | — | — | yes |
| Compress options (resolution/fps/…) | when Compress | when Compress | when Compress | — | — | when Compress |
| Advanced (attachment root, …) | — | — | name mapping | — | — | yes |

Convert/Compress need `ffmpeg`/`ffprobe` on PATH. **Do not copy** skips writing attachment files (`--media-mode disabled` / iPhone `--copy-method disabled`).

\* Required unless filled from Plus `config/owner.toml` (source-relative today); GUI collects fields explicitly.

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
| Attachments | enum | no | `--media-mode` (`clone` / `convert` / `compress` / `disabled`) |
| Max resolution / fps / min size / skip efficient | when Compress | no | `--media-max-resolution`, `--media-max-fps`, `--media-min-size`, `--media-skip-efficient` |

† At most one of `--contacts` / `--vcf`. Global Anonymize and Start/End date apply (see Shared / global controls). Convert → `.jpg`/`.mp4`/`.mp3`; Compress re-encodes (needs ffmpeg).

### SMS Backup & Restore — `sms-backup-restore-out`

Product: [SMS Backup & Restore](https://www.synctech.com.au/sms-backup-restore/)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | XML file or folder of XML | yes | `--input` |
| Output | folder | yes | `--output` |
| Your phone numbers | multi-value text | yes | `--owner-phone` |
| Contacts CSV / VCF | file | no† | `--contacts` / `--vcf` |
| Attachments | enum | no | `--media-mode` (+ compress flags; same as GO SMS Pro) |

Encrypted ZIP backups must be unlocked/extracted before selecting input. Global Anonymize and Start/End date apply.

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
| Attachments | enum | no | `--media-mode` (+ compress flags; same as GO SMS Pro) |

\* Or from crate-relative `config/owner.toml` — GUI does not rely on that; collect explicitly. Global Anonymize and Start/End date apply.

### OpenExtract — `openextract-out`

Product: [OpenExtract](https://www.openextract.app/)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | CSV file or folder | yes | `--input` |
| Output | folder | yes | `--output` |
| Contacts VCF / iMazing CSV | file | no† | `--vcf` / `--contacts` |

Global Anonymize and Start/End date apply.

### iMazing — `imazing-out`

Product: [iMazing](https://imazing.com/)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | Messages/WhatsApp CSV, chat folder, `Messages/`, `WhatsApp/`, or device export root | yes | `--input` |
| Output | folder | yes | `--output` |
| Contacts | iMazing Contacts CSV only | no | `--contacts` |
| Timezone | IANA text | no | `--timezone` (default: host local) |

Global Anonymize and Start/End date apply. WhatsApp chats write as separate `…__whatsapp.csv` files. See [`crates/imazing-out/docs/DESIGN.md`](../crates/imazing-out/docs/DESIGN.md).

### iPhone backup — `imessage-exporter`

Form link label: **imessage-exporter** → [imessage-exporter](https://github.com/ReagentX/imessage-exporter). Dropdown stays **iPhone backup**.

GUI defaults: `-f csv`, `--copy-method clone` (or `disabled`), always `--use-caller-id`. Convert/Compress run as a GUI post-step via `message-media` (not imessage `basic`/`full`).

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Database / iOS backup path | file/folder | no | `-p` / `--db-path` |
| Backup password | password | no | `-x` / `--cleartext-password` |
| Platform | macOS / iOS / auto | no | `-a` / `--platform` |
| Output / export path | folder | yes | `-o` / `--export-path` |
| Attachments | enum | no | copy `clone`/`disabled`; convert/compress post-process |
| Max resolution / fps / min size / skip efficient | when Compress | no | GUI → `message-media` compress options |
| Attachment root | folder | no | `-r` / `--attachment-root` (advanced) |
| Conversation filter | text | no | `-t` (advanced) |
| Contacts (AddressBook DB) | file | no | `-n` / `--contacts-path` (advanced) |

Global Anonymize and Start/End date apply. With Convert/Compress, anonymize runs in the GUI after media. Not exposed: `--custom-name`, `--ignore-disk-warning`. Caller ID is always on.

Advanced panel uses a chevron toggle (**Show advanced options**), not a checkbox.

## Validation rules

1. **Contacts mutual exclusion:** for Android/OpenExtract, allow at most one of `--contacts` vs `--vcf`.
2. **Contacts format:** label and file filters must match the exporter (VCF / iMazing Contacts CSV vs Apple AddressBook). Legacy vault CSV is not supported.
3. **Phone numbers:** required for GO SMS Pro and SMS Backup & Restore before Run; Plus also requires email address(es).
4. **Path existence:** input must exist; output folder may be created on run.
5. **Anonymize seed:** if provided, must be 64 hex characters; empty means generate.
6. **Timezone (iMazing):** if set, must be a valid IANA name (or defer to converter error).
7. **iPhone backup:** output directory is required; always passes `--use-caller-id`; anonymize only applies to CSV.
8. **SMS Backup+:** exactly one input path; GUI always prefixes `convert` and always passes `--verbose`.
9. **Date range:** optional start/end `YYYY-MM-DD`; end is exclusive; blank means unbounded (CLI validates).
10. **Media convert/compress:** require `ffmpeg` and `ffprobe` on PATH; Compress options validated (fps number, min size like `20M`).
11. **Warn (non-blocking):** missing contacts → same warning language as CLIs (“phones will not be resolved to names”).

## Form flow

```text
Tabs: Validate contacts | Export
  Validate → contacts file, USA checkbox → Check / Update / Cancel → shared log
  Export → pick backup source → global Anonymize/dates → per-source form → Run / Cancel → shared log
```

## Known gaps

| Gap | Detail | Suggested fix |
|-----|--------|---------------|
| Plus `owner.toml` | Resolved via `CARGO_MANIFEST_DIR`, not user cwd | GUI collects phone/email/input explicitly |
| iMessage flag style | Short flags (`-f`, `-c`, `-o`) vs long `--input` family | GUI abstracts; map to correct argv |
| iMazing attachments | Filename-only; no media copy | Document in UI; optional future media join |
| Encrypted backup password | Visible in process list if passed as argv | Prefer env/stdin if CLI grows support; warn in UI |

## Next steps

- Add application icons and native installers/packages.
- Persist recently used paths and non-secret preferences.
- Add platform CI builds and GUI smoke tests.
