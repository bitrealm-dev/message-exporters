# Message Exporters FLTK GUI

[FLTK](https://github.com/fltk-rs/fltk-rs) desktop interface for the exporter binaries in this workspace. Same workflows as the egui GUI: **Contacts** (validate) and **Message** (export).

Shared form models and process spawning live in [`message-exporters-core`](../message-exporters-core).

## Run

Build sibling tools and the GUI in the same profile:

```bash
cargo build --workspace
cargo run -p message-exporters-fltk-rs
```

Release:

```bash
cargo build --workspace --release
./target/release/message-exporters-fltk-rs
```

Binary discovery matches the egui GUI: beside the executable, then `MESSAGE_EXPORTERS_BIN`, then `PATH`.

## Linux build notes

Install X11/FLTK development libraries before building, including:

```bash
sudo apt install libxft-dev libxinerama-dev libxcursor-dev libxrender-dev libxfixes-dev
```

This crate enables fltk’s `no-pango` feature by default. For Pango-backed fonts, also install `libpango1.0-dev` / `libcairo2-dev` and drop `no-pango` from `Cargo.toml`.

## See also

- egui GUI: [`../message-exporters-gui`](../message-exporters-gui)
- Option matrix: [`../../docs/GUI.md`](../../docs/GUI.md)
