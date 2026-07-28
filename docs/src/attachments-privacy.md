# Attachments and privacy

Control whether media is copied into the export, how it is transcoded, and whether personal data is rewritten for sharing.

## Attachment modes

On the Message and Re-export tabs, set **Attachments** before Run:

| Mode | Behavior |
|------|----------|
| **Do not copy** | No media in the export |
| **Copy** (default) | Include original media files |
| **Convert** | Transcode common types to `.jpg` / `.mp4` / `.mp3` |
| **Convert & compress** | Re-encode with size/quality limits |

JSON / JSONL / CSV store media in a sidecar `attachments/` folder. EML / MBOX / XML transform media then embed it; the output folder does not keep a sidecar.

Convert and Compress require `ffmpeg` and `ffprobe` on `PATH`. Install ffmpeg before selecting those modes. See [Install](install.md).

### Compress options

When Compress is selected, set:

- **Max resolution** (for example 1080p)
- **Max fps**
- **Min size** (only re-encode files at or above this size, for example `20M`)
- **Skip efficient** when already-efficient HEVC under the max resolution should be left alone

## Obfuscate

Enable **Obfuscate** to rewrite display names, phone numbers, message text (same length), and media into stable placeholders. Use this before sharing an export for demos or support.

1. Check **Obfuscate**.
2. Optional: enter a **Seed** of exactly eight hexadecimal characters for a repeatable rewrite. Leave blank to generate a seed at run time.
3. Run the export or re-export as usual.

Obfuscate applies to every output format.

## Recommended defaults

1. Use **Copy** for a personal archive.
2. Use **Convert** or **Compress** only when the next tool needs smaller or more compatible media.
3. Enable **Obfuscate** only when the output will leave a private machine.

## Related

- [Use the desktop app](desktop-app.md)
- [Re-export between formats](reexport.md)
