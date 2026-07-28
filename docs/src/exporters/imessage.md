# Export an iPhone backup

Export Apple Messages from a local `chat.db` or an iOS backup into CSV, EML, MBOX, JSON, JSON Lines, or SyncTech XML.

## Before starting

1. Obtain a Mac Messages database (`chat.db`) **or** an unencrypted or password-known iOS backup that the exporter can open.
2. Note any backup password.
3. Create an empty **output** folder.
4. Optional: prepare an Apple AddressBook contacts database if names should resolve on macOS paths.

## Steps (desktop app)

1. Open **Message**.
2. Set **Backup type** to **iPhone backup**.
3. Select **Output format**.
4. Set **Database / iOS backup path** to the `chat.db` file or backup folder.
5. Set **Output directory**.
6. Set **Platform** (Auto, macOS, or iOS) when the default is wrong.
7. Enter **Backup password** when the iOS backup is encrypted.
8. Choose **Attachments** (Copy is the usual choice).
9. Optional: open advanced options for attachment root, conversation filter, or Apple contacts path.
10. Optional: set date range or **Obfuscate**.
11. Select **Run exporter**.
12. Review the **Log** for completion and warnings.

## Result

Per-conversation files (or one `smses.xml`) appear under the output directory. Media lands in `attachments/` when copying is enabled.

## If it fails

- Confirm the path points at a real Messages database or iOS backup.
- Confirm the backup password when the backup is encrypted.
- For convert/compress attachment modes, install `ffmpeg` / `ffprobe`. See [Attachments and privacy](attachments-privacy.md).

## Related

- [Choose an output format](../formats.md)
- [CLI man page](../reference/imessage.md)
