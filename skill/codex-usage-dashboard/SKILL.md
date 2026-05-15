---
name: codex-usage-dashboard
description: Open and manage a local, read-only Codex usage dashboard from any Codex conversation. Use when the user asks to view Codex usage, token usage, per-conversation usage, session rankings, cost estimates, quota consumption, 5-hour or weekly limit usage, or wants to launch/install a cross-platform Codex usage dashboard on Windows or macOS.
---

# Codex Usage Dashboard

## Quick Start

To open the dashboard, run the bundled opener script:

```bash
python scripts/open_dashboard.py
```

Use `python3` instead of `python` on macOS if needed:

```bash
python3 scripts/open_dashboard.py
```

The opener starts `scripts/codex_usage_dashboard.py` as a local read-only web app and opens the default browser at `http://127.0.0.1:8765/`.

## Behavior

- Reads Codex logs from `~/.codex/sessions` and `~/.codex/archived_sessions`.
- Does not modify Codex logs, state databases, auth files, or config.
- Reuses an existing dashboard process if one is already serving `127.0.0.1:8765`.
- Works on Windows and macOS with Python 3 and only standard-library modules.

## Desktop Shortcut

If the user asks for a desktop shortcut, run:

```bash
python scripts/install_desktop_shortcut.py
```

On Windows this creates `Codex Usage Dashboard.lnk`. On macOS this creates `Codex Usage Dashboard.command`.

## Notes

- The dollar cost shown by the dashboard is an estimate from public API prices and may not match ChatGPT/Codex subscription billing.
- The 5-hour and weekly quota columns use session-level deltas from Codex `rate_limits` events when available.
