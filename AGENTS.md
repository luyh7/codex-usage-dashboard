Avoid unnecessary secondary labels and descriptions. Let the layout do the explaining.

# Development Contract

- Format all dates and times using the selected display timezone, but do not show timezone names, abbreviations, or UTC offsets in page content, including cards, charts, tables, and tooltips. Keep timezone values confined to the timezone setting.
- After every code change, run the dashboard from the current worktree on port `8765` for verification. If port `8765` already hosts this project's dashboard, confirm its process identity and replace it gracefully. Do not use an alternate port. Before handoff, verify that `http://127.0.0.1:8765/api/health` reflects the current code.
