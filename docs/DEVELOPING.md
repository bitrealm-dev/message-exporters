# Developer notes

## Cutting a release

Prebuilt binaries are published only by a **manual** GitHub Actions workflow. Nothing builds or releases on push, PR, or tag by default.

Workflow file: [`.github/workflows/release.yml`](../.github/workflows/release.yml)

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

For each platform, these binaries are attached to the release:

- `go-sms-pro-to-csv`
- `sms-backup-restore-to-csv`
- `sms-backup-plus-to-csv`
- `imessage-exporter`

| Platform | Runner | Asset name suffix |
|----------|--------|-------------------|
| Linux | `ubuntu-latest` | `x86_64-unknown-linux-gnu` |
| Windows | `windows-latest` | `x86_64-pc-windows-msvc` (`.exe`) |
| macOS | `macos-latest` | `aarch64-apple-darwin` (Apple Silicon) |

Example asset: `go-sms-pro-to-csv-x86_64-unknown-linux-gnu`.

Re-running the workflow with a version that already has a tag/release will fail at `gh release create`. Bump the version or delete the old release/tag first if you intentionally want to replace it.

### Notifications

The workflow does not send email itself. GitHub may still email you about failed (or successful) Actions runs based on your account settings.

To quiet that: [Notification settings](https://github.com/settings/notifications) → **Actions** → turn off the emails you do not want. That is account-level; it cannot be forced from the workflow YAML.
