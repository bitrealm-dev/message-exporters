# Check and update contacts

Use the Contacts tab to validate a contacts file before export, or write a cleaned copy with unambiguous phone numbers normalized.

## Before starting

1. Keep `contacts-validate` in the same folder as the GUI (from the Release archive).
2. Prepare a contacts file: VCF, or a CSV with first name, last name, and a phone column (iMazing-style contacts CSV is accepted where noted).

## Check (dry run)

1. Open **Contacts**.
2. Choose the contacts **File**.
3. Leave **USA numbers** checked when numbers should be interpreted as US; clear it for international parsing.
4. Select **Check**.
5. Open **Log** and review UNCERTAIN / DUPLICATE / summary lines.

### Result

No contact file is rewritten. The log reports problems to fix by hand or with Update.

## Update (write corrected files)

1. Complete the Check setup above.
2. Select **Update**.
3. Review the **Log** for the written paths.

### Result

A sibling file is written (for example `<stem>-update.vcf` or `<stem>-update.csv`, with `-N` when re-updating), plus a `.log`. Only unambiguous phones are rewritten; uncertain values stay as-is. CSV updates may also emit a `.vcf`.

## If it fails

- Confirm `contacts-validate` is on `PATH`, beside the GUI, or under `MESSAGE_EXPORTERS_BIN`.
- Confirm the file extension matches a supported contacts format.

## After contacts are ready

Return to **Message** and attach the cleaned contacts file on exporters that accept VCF or contacts CSV. See [Use the desktop app](desktop-app.md).
