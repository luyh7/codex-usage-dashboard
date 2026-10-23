Avoid unnecessary secondary labels and descriptions. Let the layout do the explaining.

# Development Contract

- Format all dates and times using the selected display timezone, but do not show timezone names, abbreviations, or UTC offsets in page content, including cards, charts, tables, and tooltips. Keep timezone values confined to the timezone setting.
- After every code change, run the dashboard from the current worktree on an available port. Use separate ports and runtime state directories for concurrent worktrees; do not stop or replace another worktree's service.
- Before handoff, compare that instance's `/api/health` build ID with the current worktree's binary and provide its actual URL.

# Automatic Worktrees

- Before changing code for a new independent task, fetch `origin` and use Codex's worktree tools to create and attach a worktree from the latest `origin/main`, with a task-named `codex/` branch. Do this automatically without asking.
- Reuse the task's worktree for follow-up changes. Discussion and read-only tasks need no worktree.
- Run all task edits, builds, tests, and Git operations in the returned worktree directory. Leave unrelated changes and other tasks' worktrees untouched.
- If worktree setup fails, report the blocker; do not fall back to editing the main checkout.
- Worktree creation does not authorize commits, pushes, merges, or releases.
