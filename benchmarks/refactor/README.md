# Pre-refactor measurement probes

These are disposable evaluation implementations, not production code. The Rust
binary implements selected module contracts; the Svelte probe deliberately reuses
the production business calculations and markup to isolate keyed DOM updates.

Read [the measured report](../../docs/performance/refactor-evaluation.md) and
[the implementation plan](../../docs/plans/rust-svelte-refactor.md).

## Reproduce

From the repository root:

```bash
python3 benchmarks/refactor/run_all.py
```

Requirements: Python 3, Rust/Cargo, Node/npm, Linux `ss`, and a Playwright Chromium
installation under `~/.cache/ms-playwright/chromium-*/chrome-linux64/chrome`.
The first dependency installation needs network access. Lockfiles pin versions.
This harness targets the current Linux/WSL evaluation host, not a cross-platform
test runner. Windows-mounted I/O is tested when the configured temp directory
exists; missing mount results are marked unavailable. No user logs are read.

The runner builds, generates synthetic fixtures, runs the existing tests and
module comparisons sequentially, tests production HTTP on **8765**, benchmarks
the real DOM in Chromium, restores the default worktree dashboard, verifies
health and served HTML, and generates the report. It temporarily replaces this
worktree's dashboard during HTTP testing using an isolated config directory.
It refuses to stop another project's process on 8765.

Useful individual commands:

```bash
python3 benchmarks/refactor/backend.py generate
cargo build --release --locked --manifest-path benchmarks/refactor/rust/Cargo.toml
python3 benchmarks/refactor/backend.py run --samples 15
python3 benchmarks/refactor/correctness.py
python3 benchmarks/refactor/io_query.py
python3 benchmarks/refactor/http_profile.py
npm --prefix benchmarks/refactor/frontend ci
npm --prefix benchmarks/refactor/frontend run build
npm --prefix benchmarks/refactor/frontend run bench
python3 benchmarks/refactor/verify_dashboard.py --check
```

## Evidence layout

- `results/backend.json`: per-operation distributions, peak worker RSS, current
  analyzer full-scan/cache/append baselines.
- `results/correctness.json`: differential cases, exhaustive pricing grid, limits.
- `results/io-query.json`: local vs Windows-mounted I/O, buffer variant, SQL shape.
- `results/http.json`: real current-server requests, concurrent readers, remote CRUD.
- `results/cold-profile.txt`: instrumented Python call profile, not wall-time baseline.
- `results/frontend.json`: real DOM timings, two update candidates, heap and parity.
- `results/svelte-probe.png`: captured probe UI, **not a proposed redesign**.
- `results/verification.json`: successful stages, source/result hashes, fixed-port health.

`.work/`, compiled binaries, npm modules and frontend bundles are ignored. Results
contain synthetic identifiers only. Reproduction overwrites generated evidence.
CPU use, memory, and rendering measurements describe this host and these inputs;
they do not prove macOS/Windows-native/Tauri behavior or a complete Rust rewrite's
end-to-end improvement.
