# Codex Usage Dashboard Skill

**Codex-only skill for a local OpenAI Codex usage dashboard.**

This project installs a Codex skill that opens a browser dashboard for your local Codex session logs. It helps you inspect **per-conversation Codex usage**, including token totals, cost estimates, cache hit rate, reasoning effort, 5-hour quota consumption, weekly quota consumption, and conversation-level details.

> Currently only supports OpenAI Codex. It is not a Claude Code, Cursor, Continue, or generic OpenAI API dashboard.

## Why This Exists

Most usage tools show daily or project-level totals. This dashboard focuses on the thing Codex users often want to know:

**Which Codex conversation used the most?**

It reads local Codex JSONL logs and shows a searchable list of conversations with usage details for each session.

## Features

- **Per-conversation usage**: rank and inspect individual Codex conversations.
- **Recent or total sorting**: switch between recently used conversations and highest token usage.
- **Token totals**: compact list view plus detailed input, cached input, output, and reasoning tokens.
- **Estimated spend**: approximate dollar cost from public API pricing.
- **5-hour quota delta**: estimates how much of the 5-hour Codex quota a conversation consumed.
- **Weekly quota delta**: estimates how much weekly quota a conversation consumed.
- **Cache hit rate**: see how much input was cached.
- **Conversation details**: token timeline, tool call counts, model, reasoning effort, project path, and log file path.
- **Local and read-only**: reads `~/.codex/sessions` and `~/.codex/archived_sessions`; does not modify Codex files.
- **Cross-platform**: works on Windows and macOS with Python 3.
- **No API key required**: all data comes from local Codex logs.

## Install With npx

From GitHub:

```bash
npx github:<your-github-username>/codex-usage-dashboard-skill
```

After install, restart Codex or open a new Codex conversation, then ask:

```text
Use $codex-usage-dashboard to open my Codex usage dashboard
```

Install and create a desktop shortcut:

```bash
npx github:<your-github-username>/codex-usage-dashboard-skill -- --shortcut
```

Install and open immediately:

```bash
npx github:<your-github-username>/codex-usage-dashboard-skill -- --open
```

When published to npm, the shorter form will be:

```bash
npx codex-usage-dashboard-skill
```

## What Gets Installed

The installer copies this skill to:

```text
~/.codex/skills/codex-usage-dashboard
```

The skill includes:

```text
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

## Data Source

The dashboard reads local Codex logs:

```text
~/.codex/sessions
~/.codex/archived_sessions
```

It parses Codex `token_count` events, including:

- `total_token_usage`
- `last_token_usage`
- `rate_limits`
- model context window
- task completion timing
- tool calls

## Privacy

This is a local dashboard. It does not upload your Codex logs. It starts a local server bound to `127.0.0.1`.

## Limitations

- Codex-only: this skill is currently designed for OpenAI Codex Desktop/CLI local session logs.
- Cost is an estimate from public API prices and may not match ChatGPT/Codex subscription billing.
- 5-hour and weekly quota columns depend on Codex `rate_limits` events being present in local logs.
- Remote syncing across machines is not included; Windows and macOS each read their own local `~/.codex` directory.

## Keywords

OpenAI Codex usage dashboard, Codex token usage, Codex per conversation usage, Codex session usage, Codex quota tracker, Codex 5 hour limit, Codex weekly limit, Codex cache hit rate, Codex cost estimate, Codex local logs, Codex skill, OpenAI Codex skill.
