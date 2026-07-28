# Message Exporters GUI

Living design notes for the cross-platform desktop GUI that drives the existing exporter binaries.

**Framework:** [egui](https://github.com/emilk/egui) / [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) 0.31, implemented in
[`crates/message-exporters-gui`](../crates/message-exporters-gui).

## Goals

- One app for Linux, macOS, and Windows with a native look and feel.
- Drive exporters via their Rust libraries in-process (`ExporterConfig` + `run`); each crate also ships a thin standalone CLI with the same pipeline.
- Show only the controls that apply to the selected backup source; validate before run.
- Stream library log lines in the UI; support cancel (cooperative flags; WhatsApp’s external `wtsexporter` step is not killable mid-run).
- Prefer plain-language labels and product site links over CLI jargon.

## Current implementation

- Pure Rust egui/eframe desktop app for Linux, macOS, and Windows.
- Top tab panel: **Contacts** (default) | **Message** | **Re-export** | **Log**.
- Typed UI `Form` plus shared `ExporterConfig` / `SourceConfig` in `message-exporters-core` (`Form::to_config`).
- Native file/folder dialogs through `rfd`.
- Export converters are linked libraries (no sibling exporter binaries required for convert). `contacts-validate` and WhatsApp’s `wtsexporter` still resolve beside the GUI, via `MESSAGE_EXPORTERS_BIN`, or on `PATH`.
- Live tagged log and cooperative cancellation (mpsc poll in `update`).
- Exporter-specific validation before launch (`Form::to_config`), then in-process `run(&ExporterConfig)`.
- Backup-source titles link to the upstream product site.
- **Global options** (Obfuscate + Start/End date) on the Message tab per-source form.
- **Re-export** tab converts a prior output folder via `message-reexporter` (INI section `[message-reexport]`).

Export options persist in `export.ini` (load on start; save on Run / exit). Prefer an existing file in the working directory, else beside the GUI binary; otherwise create `./export.ini` on first save. Template: [`export.example.ini`](../crates/message-exporters-gui/export.example.ini). Backup passwords are never written.

Build the workspace, then run:

```bash
cargo build --workspace
# optional: cp crates/message-exporters-gui/export.example.ini export.ini
cargo run -p message-exporters-gui
```

## Non-goals

- Packaging / installers.

## Layout

1. Top tabs — **Contacts** | **Message** | **Re-export** | **Log**
2. **Contacts:** contacts file, USA numbers checkbox, Check / Update / Cancel
3. **Message:** backup source picker + global options + per-source form
4. **Re-export:** convert a prior Message Exporters output (`message-reexporter`) — input dir, output dir, output format, attachments, obfuscate. Input format is auto-detected.
5. Shared run log (Log tab / full-window log view)

### Contacts

Spawns [`contacts-validate`](../crates/message-contacts) (same discovery rules as exporters).

- **Check** (`--check`): dry run — no files written; the run log shows the same UNCERTAIN / DUPLICATE / summary content as a validate log.
- **Update**: write `<stem>-update.<ext>` (or `<stem>-update-N` when re-updating) (+ `.log`; CSV also `.vcf`). Only unambiguous phones are rewritten; uncertain values stay as-is.
- **Cancel**: stop the running job.

### Re-export — `message-reexporter`

Top tab (not a Message backup type). Converts a prior Message Exporters output folder to another packaging format (via the common message).

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input directory | folder | yes | `--input` (auto-detect csv/eml/mbox/json/jsonl/xml) |
| Output format | enum | no | `--format` |
| Output directory | folder | yes | `--output` |
| Attachments | enum | no | `--media-mode` |
| Obfuscate / seed | checkbox + text | no | `--obfuscate` / `--obfuscate-seed` |

Persists under `[message-reexport]` in `export.ini`. Mixed or unrecognized input dirs fail with a clear error. See [`crates/message-reexporter/docs/MANPAGE.md`](../crates/message-reexporter/docs/MANPAGE.md).

## Shared / global controls

| Control | Widget | CLI mapping | Notes |
|---------|--------|-------------|-------|
| Backup source | labeled selector | which binary | Supported first (iPhone backup, SMS Backup & Restore, WhatsApp), then experimental alphabetically with `(experimental)` suffix |
| Obfuscate | checkbox (global) | `--obfuscate` | When enabled, show seed field |
| Seed | text (exactly 8 hex) | `--obfuscate-seed` | Optional; blank = generate at run time |
| Start date | text (global) | `--start-date` | Optional `YYYY-MM-DD`, inclusive |
| End date | text (global) | `--end-date` | Optional `YYYY-MM-DD`, exclusive |
| Product title | hyperlink | — | Opens the upstream product/tool site |
| Input | path picker (file and/or folder) | `--input` / `-p` / etc. | Single path only |
| Output | folder picker | `--output` / `-o` | Required; choose explicitly (not derived from input) |
| Contacts | path picker | `--contacts` / `--vcf` / `-n` | Format depends on exporter; optional with warning |
| Run / Cancel | actions | in-process library `run` | Stream logs; cooperative cancel |

## Show / hide by backup source

| Section | GO SMS Pro | Backup & Restore | SMS Backup+ | OpenExtract | iMazing | WhatsApp | iPhone backup |
|---------|:----------:|:----------------:|:-----------:|:-----------:|:-------:|:--------:|:-------------:|
| Global anon + dates | yes | yes | yes | yes | yes | yes | yes |
| Input / Output | yes | yes | yes | yes | yes | output only | yes |
| DB path / Platform | — | — | — | — | — | platform (+ advanced) | primary |
| Your phone number(s) | required | required | required\* | — | — | — | — |
| Your email address(es) | — | — | required\* | — | — | — | — |
| Contacts VCF / iMazing CSV | yes | yes | yes | yes | — | — (Contacts field) | — |
| Contacts iMazing CSV | — | — | — | — | yes | — | — |
| Contacts Apple AddressBook | — | — | — | — | — | — | advanced |
| Timezone | — | — | — | — | yes | — | — |
| Name mapping | — | — | advanced | — | — | — | — |
| Verbose logging | — | — | always on | — | — | — | — |
| Output format (JSON / JSONL / CSV / EML / MBOX / XML‡) | yes | yes | yes | yes | yes | yes | yes |
| Attachments (copy/convert/compress/do not copy) | yes | yes | yes | yes†† | yes | yes | yes |
| Compress options (resolution/fps/…) | when Compress | when Compress | when Compress | — | when Compress | when Compress | when Compress |
| Advanced (attachment root, …) | — | — | name mapping | — | — | Android key / backup / wa / media / db / business | yes |

Convert/Compress need `ffmpeg`/`ffprobe` on PATH. **Do not copy** skips writing attachment files (`--media-mode disabled` / iPhone `--copy-method disabled`).

\* Required unless filled from Plus `config/owner.toml` (source-relative today); GUI collects fields explicitly.

‡ Default packaging is **JSON** (common message). Schema v3 for all exporters, including iMessage (`imessage-ir-exporter`). See [common-message.md](src/common-message.md) and [COMMON_MESSAGE.md](COMMON_MESSAGE.md). Attachment modes and obfuscate apply to every output format via `FormatSink`.

†† OpenExtract has no media in its source CSVs yet, so attachment modes are a no-op for files; the control is still shown.

## Per-exporter options

### GO SMS Pro — `go-sms-pro-exporter`

Product: [GO SMS Pro](https://play.google.com/store/apps/details?id=com.jb.gosms)

In-process via `go_sms_pro_exporter::run`. Cancel is cooperative (between XML/PDU files).

| Control | Type | Required | Library / CLI equivalent |
|---------|------|:--------:|-----|
| Input | folder (backup root with XML + PDU) | yes | `--input` |
| Output | folder | yes | `--output` |
| Your phone numbers | multi-value text | yes | `--owner-phone` (repeat) |
| Contacts CSV | file | no† | `--contacts` |
| Contacts VCF | file | no† | `--vcf` |
| Output format | enum | no | `--format` (`json` / `jsonl` / `csv` / `eml` / `mbox` / `xml`; default `json`) |
| Attachments | enum | no | `--media-mode` (`clone` / `convert` / `compress` / `disabled`); all formats via FormatSink |
| Max resolution / fps / min size / skip efficient | when Compress | no | `--media-max-resolution`, `--media-max-fps`, `--media-min-size`, `--media-skip-efficient` |

† At most one of `--contacts` / `--vcf`. Global Obfuscate and Start/End date apply for every format via FormatSink. Convert → `.jpg`/`.mp4`/`.mp3`; Compress re-encodes (needs ffmpeg).

### SMS Backup & Restore — `sms-backup-restore-exporter`

Product: [SMS Backup & Restore](https://www.synctech.com.au/sms-backup-restore/)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | XML file or folder of XML | yes | `--input` |
| Output | folder | yes | `--output` |
| Output format | enum | no | `--format` (`json` / `jsonl` / `csv` / `eml` / `mbox` / `xml`; default `json`) |
| Your phone numbers | multi-value text | yes | `--owner-phone` |
| Contacts CSV / VCF | file | no† | `--contacts` / `--vcf` |
| Attachments | enum | no | `--media-mode` (+ compress flags; same as GO SMS Pro); all formats |

Encrypted ZIP backups must be unlocked/extracted before selecting input. Builds a [common message](src/common-message.md) then projects the chosen format. Media modes and obfuscate apply through FormatSink for every format.

### SMS Backup+ — `sms-backup-plus-exporter convert`

Product: [SMS Backup+](https://github.com/jberkel/sms-backup-plus)

GUI always runs the `convert` subcommand and always passes `--verbose`.

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | one EML file or folder | yes | `--input` |
| Output | folder | yes | `--output` |
| Output format | enum | no | `--format` (`csv` / `eml` / `mbox` / `json` / `jsonl`) |
| Your phone numbers | multi-value text | yes\* | `--owner-phone` |
| Your email addresses | multi-value text | yes\* | `--owner-email` |
| Contacts CSV / VCF | file | no† | `--contacts` / `--vcf` |
| Name mapping CSV | file | no | `--name-mapping` (`Phone,Incorrect Name`) |
| Verbose | — | always | `--verbose` |
| Attachments | enum | no | `--media-mode` (+ compress flags; same as GO SMS Pro); all formats |

\* Or from crate-relative `config/owner.toml` — GUI does not rely on that; collect explicitly. Media modes and obfuscate apply through FormatSink for every format.

### OpenExtract — `openextract-exporter`

Product: [OpenExtract](https://www.openextract.app/)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | CSV file or folder | yes | `--input` |
| Output | folder | yes | `--output` |
| Output format | enum | no | `--format` (`csv` / `eml` / `mbox` / `json` / `jsonl`) |
| Contacts VCF / iMazing CSV | file | no† | `--vcf` / `--contacts` |

Media modes and obfuscate apply through FormatSink for every format. Mail is text-only (no media in source).

### iMazing — `imazing-exporter`

Product: [iMazing](https://imazing.com/)

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Input | Messages/WhatsApp CSV, chat folder, `Messages/`, `WhatsApp/`, or device export root | yes | `--input` |
| Output | folder | yes | `--output` |
| Output format | enum | no | `--format` (`csv` / `eml` / `mbox` / `json` / `jsonl`) |
| Contacts | iMazing Contacts CSV only | no | `--contacts` |
| Timezone | IANA text | no | `--timezone` (default: host local) |

Media modes and obfuscate apply through FormatSink for every format. WhatsApp chats use the `__whatsapp` stem suffix (CSV, mail, JSON, or JSONL). See [`crates/imazing-exporter/docs/DESIGN.md`](../crates/imazing-exporter/docs/DESIGN.md).

### WhatsApp — `whatsapp-exporter`

Product: [WhatsApp Chat Exporter](https://github.com/KnugiHK/WhatsApp-Chat-Exporter) (`wtsexporter`)

Requires `wtsexporter` beside the GUI, on `PATH`, in `MESSAGE_EXPORTERS_BIN`, or via `WTSEXPORTER` (pip install or release-bundled binary).

No Input directory and no Contacts file row in the GUI. `wtsexporter` runs in a temporary directory under the Output folder (so extract junk is not written into the GUI launch directory).

**iOS field order:** Backup type → Output format → Platform → Backup path → Contacts → Output → Attachments → Advanced (WhatsApp Business).

**Android field order:** Backup type → Output format → Platform → Backup path → Contacts → Output → Attachments → Decryption key → Advanced (media folder, Message Database, WhatsApp Business).

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Platform | Android / iOS | yes | `--platform` |
| Backup path | folder (iOS) or crypt file (Android) | iOS yes / Android no | `--backup` |
| Contacts | file (hint: Optional wa.db / Optional ContactsV2.sqlite) | no | `--wa` |
| Decryption key | text (Android only; not saved) | no | `--key` |
| Output | folder | yes | `--output` |
| Output format | enum | no | `--format` (`csv` / `eml` / `mbox` / `json` / `jsonl`) |
| Attachments | enum | no | `--media-mode`; all formats |
| Media folder | folder (advanced, Android only) | no | `--media` |
| Message Database | file (advanced, Android only; hint: Optional msgstore.db override) | no | `--db` |
| WhatsApp Business | checkbox (advanced) | no | `--business` |

Media modes and obfuscate apply through FormatSink for every format. Output stems use the `__whatsapp` suffix. Optional CLI `--input` (defaults to cwd for resolving `msgstore.db` / media folders) is not sent by the GUI; extraction always uses a temp dir under Output.

### iPhone backup — `imessage-ir-exporter`

Form link label: **imessage-ir-exporter** → [imessage-ir-exporter](https://github.com/bitrealm-dev/message-exporters/tree/main/crates/imessage-ir-exporter). Dropdown stays **iPhone backup**.

GUI defaults: JSON, `--copy-method clone` (or `disabled`), always `--use-caller-id`. All formats (`json` / `jsonl` / `csv` / `eml` / `mbox` / `xml`) run `imessage-ir-exporter` (`chat.db` → common message → projectors). Honors dates, conversation filter, contacts, attachment embed, and caller-id on From. Convert/Compress/obfuscate apply through FormatSink for every format (same as other exporters).

| Control | Type | Required | CLI |
|---------|------|:--------:|-----|
| Database / iOS backup path | file/folder | no | `--input` |
| Backup password | password | no | `--backup-password` |
| Platform | macOS / iOS / auto | no | `--platform` |
| Output / export path | folder | yes | `--output` |
| Output format | enum | no | `--format` (`json` / `jsonl` / `csv` / `eml` / `mbox` / `xml`; default `json`) |
| Attachments | enum | no | `--copy-method` / media mode via FormatSink |
| Max resolution / fps / min size / skip efficient | when Compress | no | compress options on FormatSink |
| Attachment root | folder | no | `--attachment-root` (advanced) |
| Conversation filter | text | no | `--conversation` (advanced) |
| Contacts (AddressBook DB) | file | no | `--contacts` (advanced) |

Media modes and obfuscate apply through FormatSink for every format. Caller ID is always on.

Advanced panel uses a chevron toggle (**Show advanced options**), not a checkbox.

## Validation rules

1. **Contacts mutual exclusion:** for Android/OpenExtract, allow at most one of `--contacts` vs `--vcf`.
2. **Contacts format:** label and file filters must match the exporter (VCF / iMazing Contacts CSV vs Apple AddressBook).
3. **Phone numbers:** required for GO SMS Pro and SMS Backup & Restore before Run; Plus also requires email address(es).
4. **Path existence:** input must exist; output folder may be created on run.
5. **Obfuscate seed:** if provided, must be exactly 8 hex characters; empty means generate.
6. **Timezone (iMazing):** if set, must be a valid IANA name (or defer to converter error).
7. **iPhone backup:** output directory is required; always passes `--use-caller-id`; obfuscate / convert / compress apply via FormatSink for every format.
8. **SMS Backup+:** exactly one input path; `SourceConfig::SmsBackupPlus` sets `verbose` / `include_summary`.
9. **Date range:** optional start/end `YYYY-MM-DD`; end is exclusive; blank means unbounded (`DateRange` on `ExporterConfig`).
10. **Media convert/compress:** require `ffmpeg` and `ffprobe` on PATH; Compress options validated (fps number, min size like `20M`).
11. **Warn (non-blocking):** missing contacts → same warning language as CLIs (“phones will not be resolved to names”).

## Form flow

```text
Tabs: Contacts | Message | Re-export | Log
  Contacts → contacts file, USA checkbox → Check / Update / Cancel → log
  Message → pick backup source → Obfuscate/dates → per-source form
         → Form::to_config → ExporterConfig → library run / Cancel → log
  Re-export → input dir → output format → output dir → media/obfuscate
           → message-reexporter::run → log
```

End-user walkthrough: [`docs/src/desktop-app.md`](src/desktop-app.md).

## Known gaps

| Gap | Detail | Suggested fix |
|-----|--------|---------------|
| Plus `owner.toml` | Resolved via `CARGO_MANIFEST_DIR`, not user cwd | GUI collects phone/email/input explicitly |
| iMazing attachments | Filename-only; no media copy | Document in UI; optional future media join |
| Encrypted backup password | Still held in memory on `AppleConfig` during run | Prefer env/stdin if CLI grows support; warn in UI |

## Next steps

- Add application icons and native installers/packages.
- Add platform CI builds and GUI smoke tests.
