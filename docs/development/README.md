# Development

[← README](../../README.md) · [Usage guide](../usage.md)

Desktop clients: [build, runtime protocol and distribution](../desktop.md) · [platform verification](../desktop-verification.md).

[CI routing, caches and platform checks](ci.md) describes the shared validation pipeline and documentation-only pushes.

## Prerequisites

- Rust stable.
- Node.js 24+ and npm for the Svelte build.
- On Windows: Visual Studio C++ Build Tools and the Windows SDK.

These tools are needed to build from source. End users can run a downloaded native executable without them.

## Build from source

```bash
git clone https://github.com/luyh7/codex-usage-dashboard.git
cd codex-usage-dashboard
npm ci --prefix web
npm run build --prefix web
cargo build --release --locked --bin codex-usage-dashboard
```

The Rust binary embeds `web/dist`. Build the frontend before running Cargo; a missing `web/dist/index.html` causes the native build to fail.

Launch on macOS/Linux:

```bash
./target/release/codex-usage-dashboard
```

On Windows, run `target\release\codex-usage-dashboard.exe`.

The application serves the dashboard at [127.0.0.1:8765](http://127.0.0.1:8765/). Follow the repository's [development contract](../../AGENTS.md) when replacing a running worktree dashboard.

## Project layout

| Path | Purpose |
| --- | --- |
| `crates/usage-core` | Log reading, normalization, pricing, and usage accounting |
| `crates/usage-server` | Local API, native launcher, snapshots, and embedded web assets |
| `web` | Svelte dashboard |
| `skills/codex-usage-dashboard` | Codex skill and runtime installation |
| `bin` | npm installation entry |
| `tools` | Release packaging and verification |
| `docs` | Product design, implementation notes, and release documentation |

The current application uses Rust and Svelte. The classic interface and Python runtime have been removed.

## Checks

Run the checks relevant to your changes:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
npm run check --prefix web
npm test --prefix web
node --test tools/release.test.mjs
```

After application code changes, run an isolated dashboard from the current worktree and verify its build identity and URL as required by [AGENTS.md](../../AGENTS.md).

Python is used only by optional historical differential checks in `tools/`, `tests/`, and `benchmarks/`. Their oracle is loaded from pinned Git history into a temporary directory; it is not an application backend. Release CI runs the differential check on each supported platform.

## Package and verify a native release

Build and package the current platform:

```bash
node tools/package_release.mjs
```

Install that local artifact without downloading a published release:

```bash
node bin/install.js --runtime-manifest dist/release/manifest.json --offline
```

Verify both installation routes on port 8765 and restore the worktree dashboard:

```bash
node tools/verify_native.mjs --replace-worktree
```

The verifier uses an empty PATH for launch, export, and stop to check that the native runtime does not invoke Python or Node.

The installed skill contains:

```text
SKILL.md
native.json
scripts/install_runtime.mjs
runtime/codex-usage-dashboard
runtime/manifest.json
```

The runtime filename ends in `.exe` on Windows. Runtime downloads use the version declared in `native.json`, with SHA256 and build-identity verification.

See [release and rollback](../implementation/release-and-rollback.md), the [1.0.1 release checklist](../releases/prepare-1.0.1.md), and the [1.0.1 release notes](../releases/v1.0.1.md).

## Design and screenshots

[Clear Mint](../design/clear-mint/README.md) is the visual reference. UI requirements and acceptance checks live in the [UI contract](ui-contract.md). README brand assets reuse the approved Soft-A SVGs.

The README screenshots were rendered from the Svelte production build using the repository's synthetic [Clear Mint fixture](../../tools/fixtures/clear_mint.mjs). They show demo data, not a user's Codex history. The English image uses translated fixture titles; the Chinese image retains the original titles.

The light and dark README covers adapt the branding to GitHub's background. They do not represent separate application themes.

Store UI verification captures under `output/verification/`, preserving the original design images. Earlier screenshot/layout scripts and browser dependency overrides are listed in the [design record](../design/clear-mint/history.md#早期验收素材).
