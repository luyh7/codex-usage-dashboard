# Usage guide

[← README](../README.md) · [简体中文](../README.zh-CN.md) · [Development](development/README.md)

## Installation

### Codex skill

```bash
npx skills add luyh7/codex-usage-dashboard -g -a codex -y
```

Invoke `$codex-usage-dashboard`, or select **Codex Usage Dashboard** in the skill picker.

Every invocation checks the native runtime against the version declared by the installed skill. A missing or mismatched runtime is downloaded from the corresponding GitHub Release, then checked for its SHA256 and build identity before launch. This check requires Node.js 18+ and a published release when a download is needed.

Updating the skill also updates its declared runtime version. The next invocation installs that matching runtime. This is not a background updater that continually checks for newer releases.

### npx installer

The repository's installer installs the skill and its native runtime:

```bash
npx github:luyh7/codex-usage-dashboard
```

Install and open immediately:

```bash
npx github:luyh7/codex-usage-dashboard -- --open
```

Install and create a desktop shortcut:

```bash
npx github:luyh7/codex-usage-dashboard -- --shortcut
```

Node.js 18+ is needed for the installer. The installed executable can subsequently run on its own.

The npm installer uses `~/.agents/skills/codex-usage-dashboard`, or `$CODEX_HOME/skills/codex-usage-dashboard` when `CODEX_HOME` is explicitly set. The Skills CLI may choose its own skill location.

### Desktop app

Desktop installers and CLI programs share [one Release per version](https://github.com/luyh7/codex-usage-dashboard/releases/latest). Choose `windows-x64-setup.exe` for Windows or `macos-arm64.dmg` for Apple Silicon Mac. See [desktop installation guide](desktop.md). Existing 1.0.1/1.1.0 desktop installations use a retired update address and need a manual full-package upgrade.

### Standalone executable

Download the binary for your platform from [GitHub Releases](https://github.com/luyh7/codex-usage-dashboard/releases/latest). The skill installer uses these same native artifacts.

| Platform | Filename contains | Notes |
| --- | --- | --- |
| Windows x64 | `cli-windows-x64` | Ends in `.exe` |
| Linux x64 | `cli-linux-x64` | glibc; built on Ubuntu 22.04 |
| macOS Intel | `cli-macos-x64` | Intel Macs |
| macOS Apple Silicon | `cli-macos-arm64` | Apple Silicon Macs |

Linux ARM, Windows ARM, and musl/Alpine are not currently included. GitHub's **Source code** archives do not contain the executable.

On Windows, save the `.exe` in a permanent directory and run it. On macOS/Linux, rename the downloaded executable to `codex-usage-dashboard`, then run:

```bash
chmod +x ./codex-usage-dashboard
./codex-usage-dashboard
```

The executable needs no Node.js, Python, or Rust. It starts the service in the background and opens [127.0.0.1:8765](http://127.0.0.1:8765/). Keep the file in place while it runs. A desktop shortcut opens this browser-based dashboard.

Release files are not code-signed or notarized, so Windows or macOS may require explicit approval through the operating system's security settings.

To verify a download, replace `DOWNLOADED_FILE` below with the actual filename, then compare its SHA256 against the entry for that **exact filename** in the release's `SHA256SUMS` file:

| System | Command |
| --- | --- |
| Windows PowerShell | `Get-FileHash ".\DOWNLOADED_FILE" -Algorithm SHA256` |
| Linux | `sha256sum "DOWNLOADED_FILE"` |
| macOS | `shasum -a 256 "DOWNLOADED_FILE"` |

`manifest.json` is used by the skill installer.

To update a manual installation, stop the running program with `--stop`, replace the executable, then launch the new version.

## Launch options

With the default npm installation path on macOS/Linux:

```bash
~/.agents/skills/codex-usage-dashboard/runtime/codex-usage-dashboard
~/.agents/skills/codex-usage-dashboard/runtime/codex-usage-dashboard --stop
```

On Windows, use `runtime/codex-usage-dashboard.exe` inside the installed skill.

| Option | Behavior |
| --- | --- |
| No option | Start or reuse the background service and open the browser |
| `--stop` | Stop the authenticated, managed dashboard |
| `--no-browser` | Start without opening the browser |
| `--serve` | Run the HTTP server in the foreground |
| `--shortcut` | Create a desktop launcher for this executable |
| `--export-snapshot` | Export local data as a compressed snapshot |
| `--codex-home <path>` | Select a Codex home; repeat for multiple homes |

The background launcher uses port **8765**. The launcher verifies managed process identity before reuse or replacement. An unrelated or unmanaged listener on that port is left running and reported as an error.

For an isolated foreground instance, set `COUSASH_CONFIG_DIR` to a separate runtime directory and use `--serve --port <port>` (`0` selects an available port). The actual URL is printed at startup. The default launcher's `--stop` operates only on its managed instance on port 8765; stop a foreground instance with Ctrl+C.

The HTTP server accepts loopback addresses only. It checks Host, Origin and browser request context, and requires a per-process write token for mutations. The dashboard handles this automatically. Local scripts can obtain the token from `GET /api/security/token` and send it as `X-Codex-Write-Token`, with `Content-Type: application/json` (or `application/octet-stream` for `/api/remotes/import-file`). Shutdown continues to require its separate launcher bearer token. Local processes are trusted; do not expose this service through an unauthenticated proxy.

Installation preserves predecessor skill directories (`cousash-open`, `cousash-export`) and obsolete files inside an existing installation. Review and remove unused copies manually; the installer does not establish ownership from filenames.

## Reading the dashboard

The overview defaults to **By project**. Switch to **By session** for a flat list, or expand project groups to inspect a main thread and its subagents.

The top metrics follow the selected date range and filters across the full result set, not just the visible page. Select a session to see its own usage, cost breakdown, timeline, and further metadata. A session group's total includes its main thread and subagents; the selected main-thread detail shows only that thread's own usage.

Preferences include English/Chinese and a saved display timezone.

## Data sources

The dashboard reads active and archived logs from each configured Codex home:

```text
~/.codex/sessions
~/.codex/archived_sessions
```

Both `.jsonl` and Codex's compressed `.jsonl.zst` histories are supported. Log compression and resumption do not cause both representations to be counted as separate usage.

In Settings → Codex log sources, use the folder picker to add Codex home folders or remove existing ones. Adding a folder preserves the currently discovered folders; use Discover automatically to restore discovery. On Windows, an accessible WSL home can be added as `\\wsl.localhost\<distribution>\home\<user>\.codex` alongside the Windows home. The browser's picker opens on the machine running the dashboard and requires a graphical desktop session; Linux also needs an XDG Desktop Portal file chooser or zenity. Explicit `--codex-home` launch options override saved sources on the next startup.

Under WSL, automatic discovery also looks for accessible Windows profiles:

```text
/mnt/c/Users/<you>/.codex/sessions
/mnt/c/Users/<you>/.codex/archived_sessions
```

To specify the roots explicitly, repeat `--codex-home`. Replace `YOUR_WINDOWS_USERNAME` with your Windows profile directory name:

```bash
~/.agents/skills/codex-usage-dashboard/runtime/codex-usage-dashboard \
  --codex-home ~/.codex \
  --codex-home "/mnt/c/Users/YOUR_WINDOWS_USERNAME/.codex"
```

The reader extracts Codex `token_count` events, cumulative and incremental token usage, model context, task timing, tool calls, and recorded service-tier changes. It does not require an API key or modify the original Codex logs.

The server defaults to `127.0.0.1`. Logs are not automatically uploaded. Exporting a snapshot creates a file for you to transfer manually.

## Data and pricing

### What the estimate means

The dollar figure estimates the API-equivalent cost of usage recorded in local Codex logs. OpenAI models use official OpenAI API prices as the estimate's basis. It does not represent subscription billing, remaining subscription quota, or a payment charge.

The pricing table is bundled with the application. It uses effective dates so earlier usage retains its historical rate, and applies model-specific short/long-context and service-tier rules. It is not a live fetch of the latest public prices.

The current implementation covers the GPT-5.6, GPT-6 Astra/Sol/Luna, and GPT-6.1 Sol families, along with special model aliases. The bundled [price table](../crates/usage-core/data/prices-v1.json) defines supported models, effective dates, context boundaries, and Fast/Ultrafast multipliers; the [pricing implementation](../crates/usage-core/src/pricing.rs) applies those rules.

Pricing references used by the project: [OpenAI API pricing](https://developers.openai.com/api/docs/pricing), [API changelog](https://developers.openai.com/api/docs/changelog), and [Ultrafast mode](https://developers.openai.com/api/docs/guides/ultrafast-mode).

### Token and agent accounting

- **Cached input is part of input.** Uncached and cached input are priced using their respective rates.
- **Reasoning is part of output.** Its usage is shown as a detail within output and is not billed a second time.
- **Forked history is removed from child usage.** Parent history inherited by a subagent is excluded from the child's totals, costs, and timeline.
- **Service-tier changes apply to the corresponding events.** When a subagent log omits the tier, it inherits the parent's tier at spawn until it records a tier change of its own.
- **Unknown pricing stays unknown or partially known.** It is not silently treated as a fully priced zero. `codex-auto-review` is shown as $0.00 by project convention; this is not a published API unit price.

### Coverage

This dashboard is designed for OpenAI Codex Desktop/CLI local session logs. It does not collect usage from Claude Code, Cursor, or generic OpenAI API applications.

Temporary Codex side chats with `ephemeral: true` do not persist the session logs this dashboard reads. They can consume model usage without appearing in its totals.

## Multi-device snapshots

### Export

On the source device, open **Settings → Device data** and choose **Full export**, or run:

```bash
~/.agents/skills/codex-usage-dashboard/runtime/codex-usage-dashboard --export-snapshot
```

The default filename looks like:

```text
cousash-mac-9f3a72c1.json.gz
```

The legacy `cousash-` prefix is retained for compatibility. Gzip compression preserves statistics, session details, and token timeline events.

Full export remains the default for syncing your own devices. It includes session summaries and local paths.

Choose **Redacted export** for sharing, issue feedback, or public statistics. It removes message previews, titles, local paths, branch names, agent labels, tool names, device names, and arbitrary extra metadata. It retains usage, model names, timestamps, costs, and session relationships with stable pseudonymous IDs.

```bash
~/.agents/skills/codex-usage-dashboard/runtime/codex-usage-dashboard --export-snapshot --redact
```

The default redacted filename is `cousash-shared.json.gz`. The private `export-redaction.key` in the application's configuration directory keeps pseudonyms stable across exports and restarts; do not include that key in shared files. Re-importing redacted exports from the same configuration updates the same sessions. Full and redacted exports deliberately have different device identities: use one mode per source device in an aggregate to avoid counting both copies. Redacted exports retain statistical timing and model information, so they are not a guarantee of complete anonymity. Local logs and the local parsed cache remain unchanged.

The dashboard reports preparation, compression, and download progress. Concurrent exports of the same mode share one job, and an unchanged local snapshot can reuse a completed export. Full and redacted jobs are kept separate.

For an older dashboard that requires uncompressed JSON, pass an explicit `.json` destination:

```bash
~/.agents/skills/codex-usage-dashboard/runtime/codex-usage-dashboard --export-snapshot snapshot.json
```

The `/api/export.json` endpoint also remains available. Add `?mode=redacted` to either export endpoint for the sharing format; omitting it keeps the full format.

### Import and manage

On the receiving device, open **Settings → Device data**, choose **Import snapshot**, and select the `.json.gz` or legacy `.json` file.

- The server decompresses gzip automatically. Both the uploaded file and decompressed JSON are limited to **512 MiB**.
- The device short code inside the snapshot is its stable identity.
- Re-importing newer data from the same device updates it incrementally by session ID.
- If the device has not been imported before, the dashboard asks for a display name.
- Imported snapshots are stored in the user's application configuration directory, outside the skill installation.
- **Rename** and **Delete** in **Device data** can rename or permanently delete an imported device.

There is no automatic cross-machine synchronization. WSL and Windows discovery on the same machine works independently of the manual snapshot workflow.

## Troubleshooting

### No sessions appear

Check the selected date range and filters, confirm that Codex has persisted local session logs, and verify the selected Codex homes. Use `--codex-home` when your logs are in a custom location.

The dashboard updates automatically as local logs change. Background checks expire directory listings after five seconds, check SQLite commits through a retained read-only connection, and audit up to eight 1 MiB log chunks every two seconds. The time for a complete background audit depends on the number and size of logs. Session-title content is checked every thirty seconds and on relevant notifications.

### The cost differs from my subscription

The estimate uses API prices, not your subscription bill or remaining quota. See [data and pricing](#data-and-pricing).

### A runtime download fails

The installed skill needs a matching release artifact for the current platform. Unsupported, missing, or invalid artifacts cause installation to fail. A repository checkout alone does not include generated binaries. For a local build, use the [development guide](development/README.md).

### Port 8765 is already in use

Use `--stop` with the existing dashboard executable to stop a managed dashboard. If another application owns the port, resolve that conflict before launching; this project uses a fixed port.

### More implementation detail

See [performance](implementation/performance.md), [compatibility](implementation/compatibility.md), and [release and rollback](implementation/release-and-rollback.md).
