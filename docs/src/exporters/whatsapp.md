# Export WhatsApp

Extract WhatsApp chats with the bundled `wtsexporter` helper, then write CSV, EML, MBOX, JSON, JSON Lines, or SyncTech XML.

## Before starting

1. Keep `wtsexporter` (or `wtsexporter.exe`) in the **same folder** as `message-exporters-gui`.
2. Gather platform-specific inputs:
   - **Android:** backup / crypt file, decryption key or crypt15 material as required by `wtsexporter`, optional media folder.
   - **iOS:** WhatsApp backup path (required).
3. Create an empty **output** folder.

## Steps (desktop app)

1. Open **Message**.
2. Set **Backup type** to **WhatsApp**.
3. Select **Output format**.
4. Select **Platform** (Android or iOS).
5. Fill the backup / key / media fields shown for that platform. Open **Show advanced options** when needed for media folder, Message Database, or WhatsApp Business.
6. Set **Output directory**.
7. Choose **Attachments**.
8. Optional: set date range or **Obfuscate**.
9. Select **Run exporter**.
10. Review the **Log**. Extraction runs through `wtsexporter` first; conversion follows.

## Result

Conversation files use a WhatsApp packaging suffix where applicable. Media is stored under `attachments/` when copying is enabled.

## If it fails

- Confirm `wtsexporter` is beside the GUI (or set `WTSEXPORTER` / `MESSAGE_EXPORTERS_BIN`).
- Confirm the crypt/key material matches the backup.
- For iOS, confirm the backup path is set.
- Cancellation cannot stop `wtsexporter` mid-run; wait for that step to finish or fail.

## Related

- [Install](../install.md)
- [Choose an output format](../formats.md)
- [CLI man page](../reference/whatsapp.md)
