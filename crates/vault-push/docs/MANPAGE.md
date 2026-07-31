# vault-push

Push a Message Exporters **JSONL** export folder into a running Message Vault (`message-vault-rs serve`).

## Synopsis

```bash
vault-push --url URL --username NAME --key TOKEN --input DIR [options]
```

Prefer `VAULT_KEY` / `VAULT_URL` environment variables over putting the key on the command line.

## Description

Reads per-conversation `.jsonl` files (message-ir schema v3) under `--input`, uploads each unique attachment by SHA-256 (`PUT /v1/assets/{sha256}`), then combines conversations into bounded vault-NDJSON message batches (`POST /v1/import`). Requests reuse HTTP connections and are flushed at the configured message count or a 16 MiB body target. Attachment uploads run concurrently. Import requests stay sequential because the server reserves one temporary import area for each account.

Progress and a durable journal (`.vault-import-state.jsonl`) live under the input directory so re-runs can resume. Secrets are never written to the journal or report.

## Options

| Flag | Env | Meaning |
|------|-----|---------|
| `--url` | `VAULT_URL` | Base URL of `message-vault-rs serve` (e.g. `http://host:8080`), **not** the Next.js UI on `:3000` |
| `--username` | | Account username |
| `--key` | `VAULT_KEY` | Per-account Import API token (Vault key) |
| `--input` | | JSONL export directory |
| `--mode append\|replace` | | Default `append` (resume-safe) |
| `--continue-on-error` | | Keep going after a failed conversation (default true) |
| `--force` | | Ignore journal; re-upload and re-import |
| `--skip-attachments` | | Import messages without uploading attachments |
| `--max-retries N` | | Transient HTTP retries (default 3) |
| `--batch-size N` | | Target messages per import request across conversations (default 1000; requests also flush near 16 MiB) |
| `--asset-upload-workers N` | | Simultaneous attachment uploads (default 4). Use `1` to disable parallel uploads. Message imports always remain sequential. |
| `--auth-only` | | Authenticate and exit |
| `--report` / `--log` / `--journal` | | Override artifact paths |

## See also

Message Exporters GUI → **Vault** tab.
