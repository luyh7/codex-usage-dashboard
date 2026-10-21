---
name: codex-usage-dashboard
description: Open the local Codex Usage Dashboard for per-conversation usage and imported device snapshots.
---

Resolve paths relative to this SKILL.md, regardless of the user's project directory.

1. Run the installed native executable: `runtime/codex-usage-dashboard` on Linux/macOS,
   or `runtime/codex-usage-dashboard.exe` on Windows. Its default action starts or
   reuses the Rust service on port 8765 and opens the original dashboard interface.
2. If the executable is missing, run `node scripts/install_runtime.mjs` from this
   skill directory to download and verify the matching published native release,
   then run the executable. Installation requires Node 18+ and internet access;
   subsequent launches require only the native executable. If no matching release
   exists, report the installer error and the source-build instructions in the
   repository README. Do not claim the dashboard started after installation failure.
3. Return the URL printed by the successful launcher. If browser opening is
   unavailable, give the user that link. An occupied, unmanaged port is an error;
   leave its process running and report the launcher's explanation.

Use `--no-browser` for a requested headless launch, `--stop` for an explicit stop,
and `--export-snapshot PATH` for a requested local-device snapshot export. Paths
containing spaces must remain one argument. The runtime requires no Python.
