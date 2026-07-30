# Third-party notices

This release archive bundles third-party binaries beside Message Exporters.
Keep these files next to `message-exporters-slint` so helper discovery works.

## wtsexporter

- Project: [KnugiHK/WhatsApp-Chat-Exporter](https://github.com/KnugiHK/WhatsApp-Chat-Exporter)
- Version: `0.13.0`
- Role: extracts WhatsApp databases for `whatsapp-exporter` / the GUI WhatsApp source
- License: MIT (see `THIRD_PARTY_WTSEXPORTER.LICENSE` in this archive)

## ffmpeg / ffprobe

- Build source: [eugeneware/ffmpeg-static](https://github.com/eugeneware/ffmpeg-static) tag `b6.1.1`
- Binary reports: FFmpeg / FFprobe `7.0.2-static` (John Van Sickle / platform build provenance in `THIRD_PARTY_FFMPEG.LICENSE`)
- Role: **Convert** and **Convert & compress** attachment modes
- License: GPL (see `THIRD_PARTY_FFMPEG.LICENSE` in this archive)

The Message Exporters project itself remains under the terms in `LICENSE`, except
where a workspace crate states otherwise (for example `imessage-ir-exporter` is
GPL-3.0-or-later via its dependencies).
