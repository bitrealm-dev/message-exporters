# Install

## Prebuilt binaries (recommended)

1. Open the latest [GitHub Release](https://github.com/bitrealm-dev/message-exporters/releases).
2. Download the archive for your OS:
   - Linux: `*-x86_64-unknown-linux-gnu`
   - Windows: `*-x86_64-pc-windows-msvc.exe`
   - macOS (Apple Silicon): `*-aarch64-apple-darwin`
3. Keep the GUI and exporter binaries in the **same folder** (the GUI finds siblings first).
4. For WhatsApp, also keep `wtsexporter` (or `wtsexporter.exe`) next to `whatsapp-exporter`—it is attached to the same release.

Then run `message-exporters-gui` (or the `.exe` on Windows).

Optional: put the folder on your `PATH`, or set `MESSAGE_EXPORTERS_BIN` to that directory.

## Build from source

You need [Rust](https://www.rust-lang.org/tools/install) (`cargo`).

```bash
git clone https://github.com/bitrealm-dev/message-exporters.git
cd message-exporters
cargo build --workspace --release
cargo run --release -p message-exporters-gui
```

Binaries land under `target/release/`.

### WhatsApp helper (`wtsexporter`)

Either use the binary from the GitHub Release, or:

```bash
pip install 'whatsapp-chat-exporter[android_backup,crypt15]'
```

Override discovery with `WTSEXPORTER=/path/to/wtsexporter`.

### Media convert / compress

`convert` and `compress` attachment modes need **ffmpeg** and **ffprobe** on `PATH`.
