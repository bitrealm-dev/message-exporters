# Export SMS Backup & Restore

Convert a SyncTech SMS Backup & Restore XML backup into CSV, EML, MBOX, JSON, JSON Lines, or SyncTech XML.

## Before starting

1. Create or unlock the SMS Backup & Restore export so a readable `.xml` (often `sms-*.xml` or similar) is on disk. Decrypt any encrypted ZIP first if required.
2. Know the **owner phone number(s)** used on that Android device (E.164 or digits). Wrong owner numbers reverse sent/received direction.
3. Optional: prepare a contacts VCF or contacts CSV so names fill in.
4. Create an empty **output** folder.

## Steps (desktop app)

1. Open **Message**.
2. Set **Backup type** to **SMS Backup & Restore**.
3. Select **Output format**.
4. Set **Input** to the XML file or the folder that contains it.
5. Set **Output directory**.
6. Enter **Your phone number(s)** (one per line or as the form requires).
7. Optional: choose a **Contacts** file.
8. Choose **Attachments**.
9. Optional: set date range or **Obfuscate**.
10. Select **Run exporter**.
11. Review the **Log**.

## Result

One artifact per conversation for the chosen format (or one merged `smses.xml` when writing XML). Media is copied into `attachments/` for JSON / JSONL / CSV, or embedded for EML / MBOX / XML.

## If it fails

- Confirm owner phones match the backup device.
- Confirm the XML is unlocked and readable.
- Missing contacts only skip name resolution; the export can still succeed.

## Related

- [Choose an output format](../formats.md)
- [Check and update contacts](../contacts.md)
- [CLI man page](../reference/sms-backup-restore.md)
