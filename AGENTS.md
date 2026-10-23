# Development Contract

- For UI changes, read and follow the [UI contract](docs/development/ui-contract.md).
- After each code change, run this worktree's dashboard on an available port with its own runtime state directory. Leave other worktrees' services running.
- Before handoff, match `/api/health`'s build ID to this worktree's binary and provide the actual URL.

# Automatic Worktrees

- For a new independent coding task, fetch `origin`, then use Codex's worktree tools to create and attach a worktree from the latest `origin/main` with a task-named `codex/` branch. Proceed without asking; reuse it for follow-ups. Read-only work needs no worktree.
- Keep all task edits, builds, tests, and Git operations in that worktree; preserve unrelated work. If setup fails, report the blocker without editing the main checkout.
- Worktree creation does not authorize commits, pushes, merges, or releases.
