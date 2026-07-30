# Message Exporters Web GUI

Living design notes for the browser-based alternative to the native
[egui desktop GUI](gui.md). Added because egui's rendering can look blurry on some
high-DPI Windows setups; the web GUI renders with the OS's own browser, so it always
looks native and crisp. There is also a [Slint desktop GUI](slint-gui.md) with the
same feature set.

**Framework:** [axum](https://github.com/tokio-rs/axum) 0.8 + [askama](https://github.com/askama-rs/askama) 0.14
server-rendered HTML, implemented in [`crates/message-exporters-web`](../../crates/message-exporters-web).

## Goals

- Same functionality as `message-exporters-gui` (Contacts, Export, Convert, Vault, Log),
  reusing the exact same `message-exporters-core` types (`Form`, `ExporterConfig`,
  `ExportIniState`, `ProcessControl`/`ProcessEvent`) and exporter library crates.
- Native browser look and feel: plain HTML forms, no client-side framework, no build step.
- Local-only: the server binds a loopback port and only ever talks to `127.0.0.1`; all
  work (exporting, validating contacts, converting, pushing to Vault) happens
  in-process on background threads with full filesystem access, exactly like the
  native GUI. Nothing is exposed to the network.
- Leave `message-exporters-gui` untouched — this is an additive alternative, not a
  replacement.

## Architecture

- `main.rs` builds an `axum::Router`, binds an OS-assigned loopback port
  (`127.0.0.1:0`), opens the user's default browser to it (via the `open` crate), and
  serves until the process is killed (`Ctrl+C`).
- `state.rs` — `AppState` holds the same `ExportIniState` + `Form` the native GUI uses
  (behind a `Mutex`, since axum handlers are `async` but the underlying work is
  synchronous/blocking), plus a `JobRegistry`.
- `runner.rs` — `JobRegistry`/`JobHandle` start each job exactly like the native GUI
  (`message_exporters_core::spawn_job`, a `std::thread` + `mpsc::Sender<ProcessEvent>`),
  then bridge that `mpsc` receiver onto a `tokio::sync::broadcast` channel plus an
  in-memory `Vec<ProcessEvent>` buffer. This lets any number of browser tabs subscribe
  to live output over Server-Sent Events (SSE), and lets a fresh page load (or
  reconnect after a network blip) replay everything emitted so far.
- `jobs.rs` — duplicated from `message-exporters-gui/src/jobs.rs` on purpose (see
  Goals above): builds the `LibraryJob` closures that call each exporter's `run(&ExporterConfig)`.
- `params.rs` — parses `axum::Form<HashMap<String, String>>` POST bodies into
  `message_exporters_core::Form` fields by hand (no `serde` derives added to the core
  `Form` type, to keep the core crate GUI-framework-agnostic).
- `views.rs` — one Askama template struct per page (`ContactsPage`, `ExportPage`,
  `ReexportPage`, `VaultPage`, `JobPage`) plus a shared `Chrome` (active nav tab +
  one-shot error banner). Routes build these; templates stay free of business logic.
- `routes/*.rs` — one file per tab (`contacts.rs`, `export.rs`, `reexport.rs`,
  `vault.rs`, `job.rs`, `browse.rs`), each a thin `axum` handler module.
- `templates/*.html` + `templates/macros.html` — server-rendered HTML. Shared field
  widgets (`text_field`, `path_field`, `select_field`, `checkbox_field`, …) are Askama
  macros; shared sections (attachment options, obfuscate options) are `{% include %}`
  partials.
- `assets/style.css`, `assets/app.js` — embedded into the binary via `include_str!`
  (no separate files to ship or serve from disk at runtime). `app.js` is a small,
  optional progressive-enhancement layer:
  1. **Live log streaming** on the job page via `EventSource` against
     `/jobs/{id}/events` (falls back to a `<noscript>` static dump of buffered lines
     plus a manual refresh link with JS disabled).
  2. **A folder/file browse dialog** (`<dialog>` + `GET /api/browse?path=…`) — there is
     no native file picker in a browser, so this lists directory contents server-side
     (the server has full filesystem access; the browser does not).

## Job execution model

Identical to the native GUI: exporters run in-process via their library `run(&ExporterConfig)`
entry point on a background `std::thread`, with a shared `CancelFlag` for cooperative
cancellation (checked between files/batches; WhatsApp's external `wtsexporter` step is
still not killable mid-run). The only difference from the native GUI is the transport
for log lines and the cancel button: SSE + an HTML `<form method="post" action="/jobs/{id}/cancel">`
instead of an in-process `mpsc` poll in `eframe::App::ui`.

## Persistence

Reads/writes the same `export.ini` format as the native GUI (see
[`ExportIniState`](../../crates/message-exporters-core/src/export_ini.rs)) — the two
GUIs can share one `export.ini` file, and switching between them keeps your last
inputs. Because a web server has no equivalent to a desktop app's clean exit hook, the
web GUI saves `export.ini` after every mutating action (exporter switch, Run, Clear)
rather than only on exit.

## Running it

```bash
cargo build --workspace
# optional: cp crates/message-exporters-gui/export.example.ini export.ini
cargo run -p message-exporters-web
```

This prints the local URL (`http://127.0.0.1:<port>/`) and opens it in your default
browser. Stop the server with `Ctrl+C`.

## Routes

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/` | Redirects to `/export` |
| GET / POST | `/contacts` | Validate/update a contacts file |
| GET | `/export` | Export form (also accepts `?exporter=<key>` to switch backup source) |
| POST | `/export` | Run the selected exporter |
| POST | `/export/clear` | Clear the active exporter's saved fields |
| GET | `/reexport` | Convert form |
| POST | `/reexport` | Run `message-reexporter` |
| POST | `/reexport/clear` | Clear convert fields |
| GET | `/vault` | Vault form |
| POST | `/vault` | Import a JSONL folder into Vault |
| POST | `/vault/authenticate` | Check Vault credentials without importing |
| POST | `/vault/clear` | Clear Vault fields |
| GET | `/jobs/latest` | Redirects to the most recently started job (nav "Log" link) |
| GET | `/jobs/{id}` | Job status page (buffered log + live SSE) |
| GET | `/jobs/{id}/events` | SSE stream of `ProcessEvent`s for a job |
| POST | `/jobs/{id}/cancel` | Cooperative cancel |
| GET | `/api/browse?path=` | JSON directory listing backing the browse dialog |
| GET | `/assets/style.css`, `/assets/app.js` | Embedded static assets |

## Known gaps

Same as the [native GUI's known gaps](gui.md#known-gaps) (they're inherited from the
shared exporter libraries), plus:

| Gap | Detail | Suggested fix |
|-----|--------|---------------|
| Single-user | `AppState` assumes one browser session at a time (e.g. exporter switch is global, not per-tab) | Fine for a local single-user tool; would need per-session state to serve multiple concurrent users |
| No auth | Anyone who can reach the loopback port can drive the app | Acceptable for local-only use; do not expose the port beyond `127.0.0.1` |
| Fixed dark theme | No light-mode toggle yet | Add a `prefers-color-scheme` media query or a theme toggle |
