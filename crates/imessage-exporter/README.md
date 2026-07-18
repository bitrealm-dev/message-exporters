# Binary Documentation

The `imessage-exporter-json` binary exports iMessage data to `txt`, `html`, or `json` formats. It can also run diagnostics to find problems with the iMessage database.

> This is a fork of [`imessage-exporter`](https://github.com/ReagentX/imessage-exporter) that adds the `json` export format. The installed command is `imessage-exporter-json`. The upstream project (without `json`) is what is published to crates.io and Homebrew as `imessage-exporter`.

Vendored in the [message-vault-rs](../..) workspace under `crates/imessage-exporter`. The SQLite parsers come from crates.io [`imessage-database`](https://crates.io/crates/imessage-database) (not vendored). For vault imports, prefer:

```bash
cargo run --release -- ingest imessage --from /path/to/iphone_backup
# or: ./scripts/ingest-staging.sh imessage
```

## Fork delta

This crate keeps the **upstream directory layout** (including `html/` and `txt/`) so merges stay path-compatible. The vault only needs JSON. Do **not** delete `html/` or `txt/` just to “trim” — that makes upstream sync harder.

| Keep as upstream baseline | Own as the JSON overlay |
|---------------------------|-------------------------|
| `src/app/` (CLI, runtime, backup/DB) | `src/exporters/json/` |
| `src/exporters/shared/` | `ExportType::Json` and `-f json` wiring |
| `src/exporters/html/`, `txt/` (paths stay on disk) | Binary name `imessage-exporter-json` |
| Most upstream CLI, docs, and deps | Default export format `json` |
| | `serde` / `serde_json` in `Cargo.toml` |

NDJSON conversation headers use `"conversation_type"` (`individual` / `group`) and iMessage `schema_version` **4**. Shared wire types live in [`message-json`](../message-json).

iOS DB / backup format fixes often land in crates.io `imessage-database` first — bump that dependency before rewriting this tree.

### Syncing from upstream

**A — Merge (preferred when paths still match)**

1. Once: `git remote add upstream https://github.com/ReagentX/imessage-exporter.git`
2. `git fetch upstream --tags`
3. Merge the desired tag into a branch that contains `crates/imessage-exporter`
4. Resolve conflicts; re-check the overlay table above
5. Smoke: `cargo build --release -p imessage-exporter`, then run `-f json` against a backup

**B — Vendor refresh (when a merge is too noisy)**

1. Snapshot `src/exporters/json/` and note Cargo.toml / bin name / default format / `ExportType` diffs
2. Replace this crate tree from the upstream release (same paths)
3. Restore the JSON overlay
4. Diff against the overlay table; smoke a JSON export
5. Bump `imessage-database` if that release requires it

## Installation

This fork is installed from source.

### Cargo (recommended)

From the **message-vault-rs** repo root:

```zsh
cargo build --release -p imessage-exporter
# optional: cargo install --path crates/imessage-exporter
```

This builds the `imessage-exporter-json` binary (`target/release/imessage-exporter-json`).

<details><summary>Uninstall steps</summary><p><pre>$ cargo uninstall imessage-exporter</pre></p><p>Optional: uninstall Rust<pre>$ rustup self uninstall</pre></p></details>

### Installing manually

- `clone` the message-vault-rs repository
- `cargo build --release -p imessage-exporter` (binary under `target/release`)

## How To Use

```txt
-d, --diagnostics
        Print diagnostic information and exit
        
-f, --format <txt, html, json>
        Specify a single file format to export messages into
        
-c, --copy-method <clone, basic, full, disabled>
        Specify an optional method to use when copying message attachments
        `clone` will copy all files without converting anything
        `basic` will copy all files and convert HEIC images to JPEG
        `full` will copy all files and convert HEIC files to JPEG, CAF to MP4, and MOV to MP4
        If omitted, the default is `disabled`
        ImageMagick is required to convert images on non-macOS platforms
        ffmpeg is required to convert audio on non-macOS platforms and video on all platforms
        
-p, --db-path <path/to/source>
        Specify an optional custom path for the iMessage database location
        For macOS, specify a path to a `chat.db` file
        For iOS, specify a path to the root of a device backup directory
        If the iOS backup is encrypted, --cleartext-password can be passed or you will be prompted for the password
        If omitted, the default directory is ~/Library/Messages/chat.db
        
-r, --attachment-root <path/to/messages/root>
        Specify an optional custom path to look for attachment data in
        Only use this if attachments are stored separately from the database's default location
        The provided path should be absolute
        This option affects both the `Attachments` and `StickerCache` directories
        Also works with jailbroken iOS sms.db databases (use `--platform macOS`)
        Has no effect on iOS backups
        The default location is ~/Library/Messages
        
-a, --platform <macOS, iOS>
        Specify the platform the database was created on
        If omitted, the platform type is determined automatically
        
-o, --export-path <path/to/save/files>
        Specify an optional custom directory for outputting exported data
        If omitted, the default directory is ~/imessage_export
        
-s, --start-date <YYYY-MM-DD>
        The start date filter
        Only messages sent on or after this date will be included
        
-e, --end-date <YYYY-MM-DD>
        The end date filter
        Only messages sent before this date will be included
        
-l, --no-lazy
        Do not include `loading="lazy"` in HTML export `img` tags
        This will make pages load slower but PDF generation work
        
-m, --custom-name <custom-name>
        Specify an optional custom name for the database owner's messages in exports
        Conflicts with --use-caller-id
        
-i, --use-caller-id
        Use the database owner's caller ID in exports instead of "Me"
        Conflicts with --custom-name
        
-b, --ignore-disk-warning
        Bypass the disk space check when exporting data
        By default, exports will not run if there is not enough free disk space
        
-t, --conversation-filter <filter>
        Filter exported conversations by contact names, numbers, or emails
        To provide multiple filter criteria, use a comma-separated string
        All conversations with the specified participants are exported, including group conversations
        Example: `-t steve@apple.com,5558675309`
        
-x, --cleartext-password <password>
        Optional password for encrypted iOS backups
        This is only used when the source is an encrypted iOS backup directory
        If omitted on an encrypted backup, you will be prompted for the password (recommended)
        A password provided with this option is visible on screen, in the process table, and in your shell history
        
-n, --contacts-path <path>
        Optional custom path for a macOS or iOS contacts database file
        This should be resolved automatically, but can be manually provided
        Handles from the messages table will be mapped to names in the provided database
        Generally, one of `AddressBook-v22.abcddb` or `AddressBook.sqlitedb`
        
    --no-progress
        Disable the on-screen progress bar regardless of context
        By default, the progress bar is shown only when stderr is a terminal,
        so headless invocations (CI, output redirected to a logfile) stay clean automatically.
        Use this flag to suppress the bar even in an interactive terminal.
        
-h, --help
        Print help
-V, --version
        Print version
```

### Examples

Export as `html` and copy attachments in web-compatible formats from the default iMessage Database location to your home directory:

```zsh
imessage-exporter-json -f html -c full
```

Export as `txt` and copy attachments in their original formats from the default iMessage Database location to a new folder in the current working directory called `output`:

```zsh
imessage-exporter-json -f txt -o output -c clone
```

Export as `json` (NDJSON) and copy attachments from an iPhone backup located at `~/iphone_backup_latest` to a new folder called `json_export`:

```zsh
imessage-exporter-json -f json -p ~/iphone_backup_latest -a iOS -o json_export -c clone
```

Export as `txt` from an iPhone backup located at `~/iphone_backup_latest` to a new folder in the current working directory called `backup_export`:

```zsh
imessage-exporter-json -f txt -p ~/iphone_backup_latest -a iOS -o backup_export
```

Export as `html` from `/Volumes/external/chat.db` to `/Volumes/external/export` without copying attachments:

```zsh
imessage-exporter-json -f html -c disabled -p /Volumes/external/chat.db -o /Volumes/external/export
```

Export as `html` from `/Volumes/external/chat.db` to `/Volumes/external/export` with attachments in `/Volumes/external/Attachments`:

```zsh
imessage-exporter-json -f html -c clone -p /Volumes/external/chat.db -r /Volumes/external/Attachments -o /Volumes/external/export 
```

Export messages from `2020-01-01` to `2020-12-31` as `txt` from the default macOS iMessage Database location to `~/export-2020`:

```zsh
imessage-exporter-json -f txt -o ~/export-2020 -s 2020-01-01 -e 2021-01-01 -a macOS
```

Export messages from a specific participant as `html` and copy attachments in their original formats from the default iMessage Database location to your home directory:

```zsh
imessage-exporter-json -f html -c clone -t "5558675309"
```

Export messages from a specific participant's name as `txt` and without attachments from the default iMessage Database location to your home directory:

```zsh
imessage-exporter-json -f txt -t "Steve Jobs"
```

Export messages from multiple specific participants as `html` without attachments from the default iMessage Database location to your home directory:

```zsh
imessage-exporter-json -f html -t "5558675309,steve@apple.com"
```

Export messages from participants matching a specific country and area code as `html` without attachments from the default iMessage Database location to your home directory:

```zsh
imessage-exporter-json -f html -t "+1555"
```

Export messages from participants using email addresses but not phone numbers as `html` without attachments from the default iMessage Database location to your home directory:

```zsh
imessage-exporter-json -f html -t "@"
```

## JSON Export

The `json` format (`-f json`) is unique to this fork. It writes **newline-delimited JSON (NDJSON)**: one `.json` file per conversation, where each line is a standalone JSON object.

- The **first line** of each file is a `conversation` record with conversation-level metadata.
- Every **subsequent line** is a `message` record.

Unlike the `html`/`txt` formats, the `json` format emits **raw handles** (phone numbers / emails) and an explicit `is_from_me` flag instead of resolved display names, so downstream tools can resolve names from their own contact source rather than parsing rendered output. Schema **v4** uses `"conversation_type"` and adds structured archival fields that HTML already had access to via `imessage-database`: read/delivered times, per-message service, deletion flag, expressive effects, edit history, multipart `parts`, best-effort balloon/app payloads, shared-location markers, attachment transcriptions, and sticker extras. Presentation-only HTML/CSS is not emitted.

Tapbacks (reactions) are attached inline to the message they react to. Reply threading is emitted as flat metadata (`thread_originator_guid` / `thread_originator_part` plus `num_replies`) so consumers can reconstruct threads themselves — each reply is also written as its own top-level `message` record, in chronological order, just like in the `html`/`txt` formats.

As with the other formats, use `-c clone` (or `basic`/`full`) to copy attachments into the export directory; the attachment `path` field is then relative to the export directory. With `-c disabled` (the default), `path` references the attachment in-place.

### Schema

`conversation` record (first line of each file):

```json
{
  "record": "conversation",
  "schema_version": 4,
  "chat_identifier": "chat1234567890",
  "service": "iMessage",
  "conversation_type": "group",
  "group_title": "Weekend Trip",
  "participants": [
    { "handle": "+15551234567", "name_hint": "Jane Doe" },
    { "handle": "person@example.com", "name_hint": "" }
  ],
  "exported_at": "2026-06-26T11:00:00-04:00"
}
```

- `conversation_type` is `"individual"` or `"group"`, derived from the chat's participants.
- `group_title` is the custom group name, or `null`.
- `participants[].handle` is the raw phone/email; `name_hint` is the exporter-resolved name (advisory; may be empty).

`message` record (every following line):

```json
{
  "record": "message",
  "guid": "0355C6E1-D0C8-4212-AA87-DD8AE4FD1203",
  "timestamp": "2026-06-26T13:45:01-04:00",
  "timestamp_utc": "2026-06-26T17:45:01+00:00",
  "timestamp_read": "2026-06-26T13:46:00-04:00",
  "timestamp_delivered": "2026-06-26T13:45:02-04:00",
  "is_from_me": false,
  "sender": "+15551234567",
  "service": "iMessage",
  "subject": null,
  "text": "Hey there!",
  "is_announcement": false,
  "announcement": null,
  "is_deleted": false,
  "expressive": "Sent with Confetti",
  "shared_location": "started",
  "balloon": { "kind": "url", "data": { "title": "Example", "url": "https://example.com" } },
  "parts": [
    {
      "index": 0,
      "kind": "run",
      "text": "Hey there!",
      "attachment_indices": [],
      "effects": [],
      "emoji_image": false
    }
  ],
  "edits": [
    {
      "part_index": 0,
      "status": "edited",
      "text": "Hey!",
      "timestamp": "2026-06-26T13:45:30-04:00",
      "timestamp_utc": "2026-06-26T17:45:30+00:00",
      "guid": null
    }
  ],
  "attachments": [
    {
      "path": "attachments/3/45.heic",
      "original_name": "IMG_0001.HEIC",
      "mime_type": "image/heic",
      "is_sticker": false,
      "transcription": "optional audio transcript",
      "genmoji_prompt": null,
      "sticker_effect": null
    }
  ],
  "tapbacks": [
    {
      "part_index": 0,
      "kind": "loved",
      "emoji": null,
      "is_from_me": true,
      "sender": null
    }
  ],
  "is_reply": false,
  "thread_originator_guid": null,
  "thread_originator_part": null,
  "num_replies": 0
}
```

- `timestamp` is RFC 3339 in the database's local time zone; `timestamp_utc` is the same instant in UTC.
- `timestamp_read` / `timestamp_delivered` are omitted when the database has no value (`0`).
- `service` is the per-message service (`iMessage`, `SMS`, `RCS`, …); omitted when unknown.
- `sender` is the raw handle of the sender, or `null` when `is_from_me` is `true`.
- `is_deleted` is `true` when the message was deleted from the conversation.
- `expressive` is the bubble/screen effect label when present; omitted otherwise.
- `shared_location` is `"started"` or `"stopped"` for legacy location-sharing markers; omitted otherwise.
- `balloon` is a best-effort structured object for URL/app/poll/placemark/etc. payloads (`kind` plus `data` when parseable); omitted for ordinary text messages.
- `parts` lists body components after attributed-body parsing (`kind` is `run`, `app`, or `retracted`). `attachment_indices` refer into this message's emitted `attachments` array. Empty when the body was not parsed into components. Omitted when empty.
- `edits` lists edit/unsend history entries (`status` is `edited`, `unsent`, or `original`). Omitted when empty.
- `attachments[].transcription` is set for audio messages with a stored transcript. `genmoji_prompt` / `sticker_effect` appear on stickers when available.
- `is_announcement`/`announcement` describe group-action messages (e.g. name changes, participant adds).
- `tapbacks` lists the reactions currently applied to the message. Each entry has a `kind` (`loved`, `liked`, `disliked`, `laughed`, `emphasized`, `questioned`, `emoji`, or `sticker`), an `emoji` (set only for `emoji` tapbacks), a `part_index` identifying which body component of the message was reacted to, and `is_from_me`/`sender` identifying the reactor. Tapbacks that were later removed are omitted, since the database only retains the latest reaction state.
- `is_reply` is `true` when the message is a thread reply. `thread_originator_guid` is the `guid` of the message it replies to and `thread_originator_part` is the targeted body component index; both are `null` for non-replies. `num_replies` is the number of replies threaded under this message.

Optional fields that are absent or empty are often omitted from the wire format (`skip_serializing_if`); consumers should treat missing keys as null/empty.

Messages that cannot be associated with a conversation are written to `orphaned.json` (without a leading `conversation` line).

## Features

[Click here](../docs/features.md) for a full list of features.

## Caveats

### Cross-platform attachment conversion

[ImageMagick](https://imagemagick.org/index.php) is required to make exported images more compatible on non-macOS platforms.

[ffmpeg](https://ffmpeg.org) is required to make exported audio more compatible on non-macOS platforms and exported video more compatible on all platforms.

### Contacts

`imessage-exporter-json` will automatically attempt to resolve handle details (email addresses and phone numbers) against contacts found either in the provided iOS backup or on the local macOS Address Book. Users can optionally pass in a path to an Address Book database, but this should generally not be necessary. (Note: the `json` format always emits the raw handle and only an advisory resolved `name_hint`, so downstream consumers can apply their own contact resolution.)

### HTML Exports

In HTML exports in Safari, when referencing files in-place, you must permit Safari to read from the local file system in the `Develop > Developer Settings...` menu:

![](../docs/binary/img/safari_local_file_restrictions.png)

Further, since the files are stored in `~/Library`, you will need to grant your browser Full Disk Access in System Settings.

Note: This is not required when passing a valid `--copy-method`.

#### Custom Styling for HTML Exports

You can customize the appearance of HTML exports by creating your own CSS file:

1. Create a file named `style.css` in the same directory as your exported files
2. Add your custom styles to this file
3. These styles will be automatically applied to your exported HTML files

Since custom styles are loaded after the default styles, they should automatically override rules with the same specificity.

##### Example Custom CSS

For example, to prevent messages from breaking across pages when printing:

```css
.message {
    break-inside: avoid;
}
```

The default styles can be viewed [here](src/exporters/html/resources/style.css).

### PDF Exports

I could not get PDF export to work in a reasonable way. The best way for a user to do this is to follow the steps above for Safari and print to PDF.

#### `wkhtmltopdf`

`wkhtmltopdf` refuses to render local images, even with the flag enabled like so:

```rust
let mut process = Command::new("wkhtmltopdf")
.args(&vec![
    "--enable-local-file-access".to_string(),
    html_path,
    pdf_path.to_string_lossy().to_string(),
])
.spawn()
.unwrap();
```

This persisted after granting `cargo`, `imessage-exporter`, and `wkhtmltopdf` Full Disk Access permissions as well as after copying files to the same directory as the `HTML` file.

#### Browser Automation

There are several `chomedriver` wrappers for Rust. The ones that use async make this binary too large (over `10mb`) and have too many dependencies. The sync implementation in the `headless-chrome` crate works, but [times out](https://github.com/atroche/rust-headless-chrome/issues/319) when generating large `PDF`s, even with an extreme timeout.
