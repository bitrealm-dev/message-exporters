# Development and releases

End-user documentation lives in the [Starlight source](../src/content/docs/) (start with [What’s inside an export](../src/content/docs/understand-output/export-structure.md)). Use the [maintainer index](README.md) to find architecture, GUI, exporter, and format documentation.

## Cutting a release

Prebuilt binaries are published only by a **manual** GitHub Actions workflow. Nothing builds or releases on push, PR, or tag by default.

Workflow file: [`.github/workflows/release.yml`](../../.github/workflows/release.yml)

### Steps

1. Merge whatever should ship onto `main` (or the branch you intend to build; the workflow checks out the branch you select when you run it).
2. Open [Actions → Release](https://github.com/bitrealm-dev/message-exporters/actions/workflows/release.yml).
3. Click **Run workflow**.
4. Choose the branch to build from (usually `main`).
5. Enter a semantic version **without** a leading `v`, for example `0.1.0`.
6. Wait for all three OS jobs (Linux, Windows, macOS) to finish and for the release job to create the GitHub Release.
7. Confirm the release at [Releases](https://github.com/bitrealm-dev/message-exporters/releases). The tag will be `v` plus your version (`0.1.0` → `v0.1.0`).

You need write access to the repository (to run workflows that create releases and tags).

### What gets published

For each platform, these binaries are attached to the release (standalone CLIs; the GUI links the exporter crates as libraries and does not need sibling exporter binaries for convert):

- `go-sms-pro-exporter`
- `sms-backup-restore-exporter`
- `sms-backup-plus-exporter`
- `openextract-exporter`
- `imazing-exporter`
- `imessage-ir-exporter`
- `whatsapp-exporter`
- `message-reexporter` (built from package `message-ir`)
- `wtsexporter` / `wtsexporter.exe` (KnugiHK 0.13.0, still required beside the GUI for WhatsApp extract)

| Platform | Runner | Asset name suffix |
|----------|--------|-------------------|
| Linux | `ubuntu-latest` | `x86_64-unknown-linux-gnu` |
| Windows | `windows-latest` | `x86_64-pc-windows-msvc` (`.exe`) |
| macOS | `macos-latest` | `aarch64-apple-darwin` (Apple Silicon) |

Example asset: `go-sms-pro-exporter-x86_64-unknown-linux-gnu`.

Re-running the workflow with a version that already has a tag/release will fail at `gh release create`. Bump the version or delete the old release/tag first if you intentionally want to replace it.

### Notifications

The workflow does not send email itself. GitHub may still email you about failed (or successful) Actions runs based on your account settings.

To quiet that: [Notification settings](https://github.com/settings/notifications) → **Actions** → turn off the emails you do not want. That is account-level; it cannot be forced from the workflow YAML.

## Documentation site (GitHub Pages)

User-facing docs use [Astro Starlight](https://starlight.astro.build/) under [`docs/`](..), deployed by [`.github/workflows/docs.yml`](../../.github/workflows/docs.yml).

### Enable Pages (one-time)

1. Repo **Settings → Pages**.
2. **Build and deployment → Source** → **GitHub Actions** (not “Deploy from a branch”).
3. Push to `main` or run the **Docs** workflow under **Actions**.
4. Site URL: `https://bitrealm-dev.github.io/message-exporters/`.

Local preview:

```bash
cd docs
npm ci
npm run dev
```

Run `npm run check` and `npm run build` before publishing documentation changes.
