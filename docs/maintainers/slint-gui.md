# Message Exporters Slint GUI

Living design notes for the Slint-based desktop alternative to the native
[egui GUI](gui.md) and the [web GUI](web-gui.md).

**Framework:** [Slint](https://slint.dev) 1.17, implemented in
[`crates/message-exporters-slint`](../../crates/message-exporters-slint).

## Goals

- Same functionality as `message-exporters-gui` (Contacts, Export, Convert, Vault, Log),
  reusing `message-exporters-core` (`Form`, `ExporterConfig`, `ExportIniState`,
  `ProcessControl` / `ProcessEvent`) and the exporter library crates.
- Dense desktop form layout (fixed label column, compact spacing, no vertical
  stretch on ordinary fields) using Slint's platform `native` widget style.
- Leave `message-exporters-gui` and `message-exporters-web` untouched — this is
  an additive alternative.

## Widget style

Compiled with Slint's `native` style in `build.rs`:

| Platform | Style |
|----------|-------|
| Windows | Fluent |
| macOS | Cupertino |
| Linux | Qt when Qt 5.15+ is available; otherwise Fluent |

This crate stays pure Rust (no Qt SDK dependency). On Linux without Qt, Fluent is
the intentional fallback. Override at compile time with `SLINT_STYLE`
(for example `SLINT_STYLE=fluent cargo build -p message-exporters-slint`).

Layout density lives in `ui/widgets.slint` (`FormMetrics`, horizontal `FormRow`
fields, ~1px row gaps). Ordinary fields do not stretch; only the Log tab's viewer
grows when the window is resized vertically. Fluent widgets still use ~32px
control chrome on Linux without Qt — density comes from packing rows tightly at
the top, not from shrinking the Fluent controls themselves.

## Architecture

- `ui/app-window.slint` — root window with `TabWidget`, error banner, status bar,
  and an About dialog that hosts Slint's `AboutSlint` widget (Royalty-free
  license attribution).
- `ui/pages/*.slint` — one page per tab; each exports a `*Adapter` global for
  properties and callbacks.
- `ui/widgets.slint` — dense form rows (`LabeledLineEdit`, `LabeledPathField`,
  `LabeledComboBox`, `AdvancedSection`, …) with a fixed label column.
- `src/main.rs` — constructs `AppWindow`, wires adapter callbacks, runs the
  event loop; persists `export.ini` on exit.
- `src/state.rs` — `AppState` holding `ExportIniState` + `Form` + job control
  (behind `Arc<Mutex<_>>` so the log bridge thread can wake the UI).
- `src/jobs.rs` — duplicated in-process `LibraryJob` dispatch (same pattern as
  the egui and web GUIs so those crates stay untouched).
- `src/sync.rs` — push `AppState` into Slint adapters / pull adapter values back
  into `Form` before validate/save.
- `src/browse.rs` — `rfd` file/folder dialogs on a background thread, results
  applied via `Weak::upgrade_in_event_loop`.
- `src/session_log.rs` — timestamped temp-file session log (same naming as egui).

Jobs still run via `message_exporters_core::spawn_job` on a `std::thread` with a
`CancelFlag` + `mpsc::Sender<ProcessEvent>`. A bridge thread drains the receiver
and marshals each line onto the Slint UI thread (`upgrade_in_event_loop`),
appending to the Log tab's `VecModel` and the session log file.

## Persistence

Reads/writes the same `export.ini` as the other GUIs via
`ExportIniState::load_or_default()` / `save()`. Saved after exporter switch /
Run / Clear, and again when the window exits.

## Licensing

Slint is used under the **Royalty-free** license. The About dialog displays the
`AboutSlint` widget to satisfy the attribution requirement. No registration or
paid commercial license is required for this desktop app.

## Running it

```bash
cargo build --workspace
cargo run -p message-exporters-slint
```

## Known gaps

Same as the [egui GUI's known gaps](gui.md#known-gaps) (inherited from shared
exporter libraries). Interactive GUI smoke tests still need a display; CI verifies
compile/link only (same constraint as `message-exporters-gui`).
