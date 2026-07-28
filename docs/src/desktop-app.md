# Use the desktop app

The desktop app exports phone message backups through the [common message](common-message.md) to JSON (default), JSON Lines, CSV, EML, MBOX, or SyncTech XML, and can re-export a prior output folder to another format.

## Tabs

| Tab | Purpose |
|-----|---------|
| **Contacts** | Check or rewrite contact files before export |
| **Message** | Export from a phone backup or vendor export |
| **Re-export** | Convert an existing Message Exporters output folder |
| **Log** | Full-window view of the latest run log |

## First export (Message tab)

1. Open **Message**.
2. Select **Backup type** (iPhone backup, SMS Backup & Restore, or WhatsApp for supported paths).
3. Select **Output format** (default **JSON**).
4. Choose input paths required for that source (backup folder, XML, database, and so on).
5. Choose an **Output directory**. Create a new empty folder for the first run.
6. Optionally set **Attachments**, date range, and **Obfuscate**.
7. Select **Run exporter**.
8. Open **Log** (or wait for the log view) and confirm the run finished without errors.

### Result

The output directory contains one artifact per conversation for the chosen format (or one `smses.xml` for XML), plus `attachments/` when media was copied.

### If it fails

- Read the log lines for missing paths, owner phone, or helper errors.
- Confirm the backup type matches the files on disk.
- For WhatsApp, confirm `wtsexporter` sits beside the GUI. See [Export WhatsApp](exporters/whatsapp.md).

## Optional Message settings

- **Start date** / **End date** — local calendar bounds (`YYYY-MM-DD`; end exclusive).
- **Obfuscate** — rewrite names, numbers, text, and media before sharing. Optional **Seed**: exactly eight hex characters.
- **Attachments** — Do not copy / Copy / Convert / Compress. Details: [Attachments and privacy](attachments-privacy.md).

## Saved options

Form values persist in `export.ini` (load on start; save on Run and on exit). Prefer a file in the working directory, else beside the GUI binary. Backup passwords are never written to the file.

Template: [`export.example.ini`](https://github.com/bitrealm-dev/message-exporters/blob/main/crates/message-exporters-gui/export.example.ini).

## Cancel

While a job runs, use **Cancel** when shown. Cancellation is cooperative for in-process exporters. WhatsApp’s external `wtsexporter` step cannot be killed mid-run from the GUI.

## Related guides

- [Export an iPhone backup](exporters/imessage.md)
- [Export SMS Backup & Restore](exporters/sms-backup-restore.md)
- [Export WhatsApp](exporters/whatsapp.md)
- [Check and update contacts](contacts.md)
- [Re-export between formats](reexport.md)
