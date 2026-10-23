---
name: codex-usage-dashboard
description: Open the local Codex Usage Dashboard for per-conversation usage and imported device snapshots.
---

Resolve paths relative to this SKILL.md, regardless of the user's project directory.

1. From this skill directory, run `node scripts/install_runtime.mjs --ensure`.
   This verifies the installed binary against `native.json` and its manifest,
   downloading the matching release when missing, outdated or still using the
   classic interface. Run this check after skill updates too; an existing executable
   alone does not prove it matches the skill. Node 18+ is required for this check;
   a download also requires internet access. On failure, report the installer error
   and the source-build instructions in the repository README; do not launch an old
   runtime or claim the dashboard started.
2. Run the verified native executable printed by the check:
   `runtime/codex-usage-dashboard` on Linux/macOS, or
   `runtime/codex-usage-dashboard.exe` on Windows. Its default action starts or
   reuses the Rust service on port 8765 and opens the dashboard at
   `http://127.0.0.1:8765/`.
3. Return the URL printed by the successful launcher. If browser opening is
   unavailable, give the user that link. An occupied, unmanaged port is an error;
   leave its process running and report the launcher's explanation.

Use `--no-browser` for a requested headless launch, `--stop` for an explicit stop,
and `--export-snapshot PATH` for a requested local-device snapshot export. Paths
containing spaces must remain one argument. Direct native launches require no
Python, Node or Rust toolchain.
