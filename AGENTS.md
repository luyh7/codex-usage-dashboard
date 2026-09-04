# Development Contract

- After every code change, run the dashboard from the current worktree on port `8765` for verification. If port `8765` already hosts this project's dashboard, confirm its process identity and replace it gracefully. Do not use an alternate port. Before handoff, verify that `http://127.0.0.1:8765/api/health` reflects the current code.
