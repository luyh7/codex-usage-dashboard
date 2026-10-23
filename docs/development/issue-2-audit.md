# Issue #2: current native implementation audit

Reviewed on 2026-10-23 against `origin/main` at `9198496`, plus the changes in `codex/audit-security-hardening`. Only the Rust runtime and current Node installer are in scope. The retired Python implementation was not run or repaired.

## Changes

- Bind only to loopback addresses. Validate the listener's Host/port, any supplied Origin, and browser request context before serving data or static assets.
- Require a random 256-bit per-process write token and the expected content type for mutations, including export preparation and binary snapshot imports. The frontend obtains a fresh token before each write so a server restart does not strand open tabs. Shutdown retains a separate bearer token.
- Return no-store, no-sniff, anti-framing and no-referrer headers. The token endpoint receives the same request checks and never enables CORS.
- Preserve predecessor skill directories, links and obsolete installation files. Remove the old filename-based cleanup as well as recursive predecessor-directory deletion.
- Permit a foreground instance on a separate port with an explicit runtime directory. Background launcher behavior stays on 8765. This enables isolated verification without replacing other worktrees' services.

This is a local browser security boundary, not authentication between local OS users. Local processes can still access the API. Remote listeners are rejected; an unauthenticated reverse proxy is unsupported.

## Disposition of the original findings

| Finding | Current result | Evidence / remaining work |
| --- | --- | --- |
| 1. HTTP API protection | Fixed in this worktree | Real HTTP tests reject hostile Host/Origin, cross-site requests, absent/wrong write tokens, text/plain bodies and invalid shutdown credentials. Authorized import, binary import, rename, delete and export preparation work. Rejected deletion preserves the stored bytes. |
| 2. PowerShell quoting | Old Python finding retired; native equivalent passes | Rust uses single-quoted PowerShell literals and doubles embedded quotes. A real temporary shortcut under paths containing `$(1+1)` and an apostrophe retains the exact executable path. |
| 3. Installer deletion | Fixed in this worktree | A real offline installation preserves modified predecessor contents, a directory junction/symlink and its target, modified obsolete scripts, and files under an old `__pycache__` directory. |
| 4. Unrelated process termination | Old matching/force-kill path removed | The native launcher checks app/protocol, PID, build ID and instance ID against its own process record, then requests bearer-authenticated shutdown. An unknown service is left running; no force-kill fallback. Verified by source inspection; the existing user's port 8765 service was not stopped or replaced for this audit. |
| 5A. Changed ctime with unchanged size/mtime | Native implementation passes tested case | FileStamp includes change time and identity in the in-memory and persisted cache. Rust's preserved-mtime rewrite/archive regression passes. An independent synthetic live-server rewrite changed total usage from 480 to 630 as expected. |
| 5B. Fully identical metadata / directory discovery | Fixed with bounded background validation and explicit refresh | Directory listings expire after five seconds. Persisted 1 MiB fingerprints support round-robin content validation (up to eight chunks every two seconds), watcher-triggered checks and full validation on demand. Component signatures include the content digest. Deterministic collision tests cover missed creates, rewritten logs, interior chunks, immutable old publications and persistent reuse after restart. A complete audit's latency scales with history size; this is not an instantaneous reread of all logs. |
| 5C. SQLite updates | Fixed | A retained read-only SQLite connection compares PRAGMA data_version between polls and reopens on file identity changes. Tests cover committed same-size WAL updates without watcher hints and unchanged polls. Force refresh rereads runtime evidence and remote snapshots before replying after publication. |
| 6. Export/cache privacy | Optional redacted export implemented; full export retained by user choice | Full export stays the default with a short summary/path notice. Redaction removes private text and paths, uses persistent keyed pseudonyms, and retains statistics and relationships. Tests cover private-field canaries, stable IDs across processes, separate gzip export jobs and repeated import without double-counting. Local logs/cache stay intact; redaction is a sharing format, not local encryption or complete anonymity. |
| 7. Price matching / reasoning | Existing matching retained by explicit user choice | Only an official OpenAI API price estimate statement is added. Matching and numerical calculations are unchanged; no match-type or assumed-rate labels are introduced. The existing broad-family matching remains an estimate rather than proof of the billed model. Reasoning is included in output and is not billed twice. |
| 8. Optional installation failures | Current installer propagates child failure | execFileSync exceptions reach the installer error handler. A fresh offline Windows installation with PowerShell unavailable returns failure for `--shortcut`. Python suitability checks are no longer applicable. Browser-opening fallback is still best-effort native behavior. |

## Verification

Run from this worktree, with an independently built binary:

```powershell
npm ci --prefix web
npm run build --prefix web
cargo build -p usage-server
cargo test --workspace
npm run check --prefix web
npm test --prefix web
node --test tools/security.test.mjs
```

The HTTP tests start the native binary on port 0 with synthetic logs and private state beneath `target/audit-tests`. Installation tests use a local checksum/build-verified manifest and fresh target directories. No real conversation data is exported by these tests. Existing services are not replaced. Additional native cache/pricing probes used synthetic JSONL and SQLite fixtures only.

The requested follow-up is implemented in this worktree. This is not a release, and the pricing policy is an accepted product decision rather than a stricter matching fix. No commit, push, release, or GitHub issue update is part of this worktree's verification.
