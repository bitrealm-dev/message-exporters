# Install

Install the desktop app from a GitHub Release so Message Exporters and its helper tools stay in one folder.

## Download

1. Open the latest [GitHub Release](https://github.com/bitrealm-dev/message-exporters/releases).
2. Download the archive for the machine:
   - **Linux:** `*-x86_64-unknown-linux-gnu`
   - **Windows:** `*-x86_64-pc-windows-msvc` (`.exe` files)
   - **macOS (Apple Silicon):** `*-aarch64-apple-darwin`
3. Extract the archive to a permanent folder (for example under Documents).
4. Keep every extracted file in that **same folder**. Do not move the GUI away from the helpers.

Helpers that still run as separate programs:

| Helper | Needed for |
|--------|------------|
| `contacts-validate` | Contacts tab Check / Update |
| `wtsexporter` | WhatsApp extraction |

Export converters for other backup types run inside the GUI; keeping them in the folder is still useful for standalone CLI use.

## Run the desktop app

1. Open the install folder.
2. Start `message-exporters-gui` (Windows: `message-exporters-gui.exe`).
3. If the OS blocks an unsigned download, allow the app once through the security prompt (SmartScreen / Gatekeeper), then start it again.

Optional:

- Add the install folder to `PATH`, or
- Set `MESSAGE_EXPORTERS_BIN` to that folder so helpers are found when the working directory differs.

## WhatsApp helper

Keep `wtsexporter` (or `wtsexporter.exe`) next to the GUI. It is attached to the same Release.

To override discovery, set `WTSEXPORTER` to the full path of the helper binary.

## Convert and compress media

Attachment **Convert** and **Compress** need `ffmpeg` and `ffprobe` on `PATH`. Install a current ffmpeg build for the OS before using those modes. See [Attachments and privacy](attachments-privacy.md).

## Result

The desktop app opens with tabs **Contacts**, **Message**, **Re-export**, and **Log**. Continue with [Use the desktop app](desktop-app.md).

Build-from-source instructions for contributors are under [Developing / releases](developing.md).
