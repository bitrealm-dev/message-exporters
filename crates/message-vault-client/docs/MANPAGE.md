# vault-push

Push a Message Exporters **JSONL** export folder into a running Message Vault (`message-vault-rs serve`).

## Synopsis

```bash
vault-push --url URL --username NAME --key TOKEN --input DIR [options]
```

Prefer `VAULT_KEY` / `VAULT_URL` environment variables over putting the key on the command line.

## Description

Reads per-conversation `.jsonl` files (message-ir schema v3) under `--input`, uploads each unique attachment by SHA-256 (`PUT /v1/assets/{sha256}`), then imports bounded vault-NDJSON message batches (`POST /v1/import`).

Progress and a durable journal (`.vault-import-state.jsonl`) live under the input directory so re-runs can resume. Secrets are never written to the journal or report.

## Options

| Flag | Env | Meaning |
|------|-----|---------|
| `--url` | `VAULT_URL` | Vault base URL |
| `--username` | | Account username |
| `--key` | `VAULT_KEY` | Per-account Import API token (Vault key) |
| `--input` | | JSONL export directory |
| `--mode append\|replace` | | Default `append` (resume-safe) |
| `--continue-on-error` | | Keep going after a failed conversation (default true) |
| `--force` | | Ignore journal; re-upload and re-import |
| `--max-retries N` | | Transient HTTP retries (default 3) |
| `--batch-size N` | | Messages per import request (default 100) |
| `--auth-only` | | Authenticate and exit |
| `--report` / `--log` / `--journal` | | Override artifact paths |

## See also

Message Exporters GUI → **Vault** tab.
