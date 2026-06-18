# Codex Usage Dashboard

Codex-only skill for a local OpenAI Codex usage dashboard.

It reads your local Codex session logs and opens a browser dashboard for per-conversation usage: total tokens, output tokens, estimated dollar cost, cache hit rate, turns, model, reasoning effort, tool calls, and token-count timeline. When launched from WSL, it can also read the mounted Windows Codex log directory and merge both environments into one dashboard.

> Currently only supports OpenAI Codex. It is not a Claude Code, Cursor, Continue, or generic OpenAI API dashboard.
<img width="2594" height="1292" alt="image" src="https://github.com/user-attachments/assets/2b6de5bd-9e81-495d-aa82-4634cf0e86a9" />

## Why This Exists

Most usage tools show daily or project-level totals. This dashboard focuses on the thing Codex users often want to know:

**Which Codex conversation used the most?**

It shows a searchable conversation list and a detail view for each session, including the cost split for input, cached input, output, and reasoning tokens.

## Install With The Skills CLI

Recommended global Codex install:

```bash
npx skills add luyh7/codex-usage-dashboard -g -a codex -y
```

Then restart Codex or open a new Codex conversation. Type `/usage`, then select the **Codex Usage Dashboard** skill.

The skill opens the local dashboard in your browser.

## Install With npx

GitHub installer:

```bash
npx github:luyh7/codex-usage-dashboard
```

Install and create a desktop shortcut:

```bash
npx github:luyh7/codex-usage-dashboard -- --shortcut
```

Install and open immediately:

```bash
npx github:luyh7/codex-usage-dashboard -- --open
```

## Features

- Per-conversation Codex usage from local session logs.
- Default recent sorting, with a prominent switch to sort by total tokens.
- Compact list columns for total tokens, output tokens, cost, cache hit rate, turns, model, and reasoning effort.
- Detail view with token breakdown and estimated dollar cost for input, cached input, output, and reasoning.
- Token-count timeline with latest entries shown first.
- Tool call counts, project path, log file path, and session metadata.
- Local and read-only: reads Codex session logs from one or more local Codex homes; does not modify Codex logs.
- WSL + Windows support: when running in WSL, automatically merges WSL `~/.codex` with Windows `/mnt/c/Users/<you>/.codex` when present.
- Cross-platform: works on Windows and macOS with Python 3.
- No API key required.

## What Gets Installed

The standard skills CLI installs:

```text
~/.codex/skills/codex-usage-dashboard
```

The bundled npm installer copies the same skill to that location.

Repository layout:

```text
skills/
  codex-usage-dashboard/
    SKILL.md
    agents/openai.yaml
    scripts/open_dashboard.py
    scripts/codex_usage_dashboard.py
    scripts/install_desktop_shortcut.py
    assets/codex_usage_dashboard.ico
```

## Manual Launch

After installation:

```bash
python ~/.codex/skills/codex-usage-dashboard/scripts/open_dashboard.py
```

On macOS you may need:

```bash
python3 ~/.codex/skills/codex-usage-dashboard/scripts/open_dashboard.py
```

The dashboard opens at:

```text
http://127.0.0.1:8765/
```

To force specific log roots, pass `--codex-home` one or more times:

```bash
python ~/.codex/skills/codex-usage-dashboard/scripts/codex_usage_dashboard.py \
  --codex-home ~/.codex \
  --codex-home /mnt/c/Users/<you>/.codex
```

## Data Source

The dashboard reads local Codex logs from each configured Codex home:

```text
~/.codex/sessions
~/.codex/archived_sessions
```

Under WSL, the default scan also includes:

```text
/mnt/c/Users/<you>/.codex/sessions
/mnt/c/Users/<you>/.codex/archived_sessions
```

It parses Codex `token_count` events, including `total_token_usage`, `last_token_usage`, model context window, task timing, and tool calls.

## Privacy

This is a local dashboard. It does not upload your Codex logs. It starts a local server bound to `127.0.0.1`.

## Limitations

- Codex-only: designed for OpenAI Codex Desktop/CLI local session logs.
- Cost is an estimate from public API prices and may not match ChatGPT/Codex subscription billing.
- Remote syncing across machines is not included. WSL + Windows works on the same machine when the Windows profile is mounted under `/mnt/c`.

## Keywords

OpenAI Codex usage dashboard, Codex token usage, Codex per conversation usage, Codex session usage, Codex output tokens, Codex cache hit rate, Codex cost estimate, Codex local logs, Codex skill, OpenAI Codex skill.
