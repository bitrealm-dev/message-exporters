# Message Exporters iced GUI

[iced](https://iced.rs) 0.14 desktop interface for the exporter binaries in this workspace. Same workflows as the egui and FLTK GUIs: **Contacts** (validate) and **Message** (export).

Shared form models and process spawning live in [`message-exporters-core`](../message-exporters-core).

## Run

Build sibling tools and the GUI in the same profile:

```bash
cargo build --workspace
cargo run -p message-exporters-iced-rs
```

Release:

```bash
cargo build --workspace --release
./target/release/message-exporters-iced-rs
```

Binary discovery matches the other GUIs: beside the executable, then `MESSAGE_EXPORTERS_BIN`, then `PATH`.

## See also

- egui GUI: [`../message-exporters-gui`](../message-exporters-gui)
- FLTK GUI: [`../message-exporters-fltk-rs`](../message-exporters-fltk-rs)
- Option matrix: [`../../docs/GUI.md`](../../docs/GUI.md)
