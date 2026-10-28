# Browser / core compatibility contract

Baseline: `c42f0e8ae948d4cde392fafa9894681a1200b17d`, production Python SHA256
`20fae68140fa8186ed86728aaac16d8d6688b1218d555e93e1a49e4761319135`.
Both matched the pre-refactor report before implementation. The original 90 tests
passed. Existing `benchmarks/refactor/results` evidence is retained unchanged.

## Golden replay

`python3 tools/compatibility.py` replays the JSON cases in
`tests/fixtures/compatibility`. Inputs are synthetic tests, encoded as exact file
bytes (base64) plus file mtime nanoseconds; runtime SQLite fixtures contain SQL
from a synthetic database. Neither collection nor replay scans user log roots.
The independent recursive comparator requires identical key sets, list ordering,
scalar types, integer values, nulls, and six-decimal money. It does not ignore
entire response trees or use a numeric tolerance.

The 13 frozen cases cover continued files, parent inheritance, spawn tier,
zero baseline, reset, missing parent, runtime tier changes, per-event price
segments, multiple agent levels, tool counts, title filtering and archive moves.
Cases from legacy tests retain the originating test name and its hand assertions.
The nested synthetic case is an additional full-field regression contract.

Only these differences are normalized:

- Absolute synthetic Codex root prefixes become `<HOME:n>`; relative filenames
  are unchanged. Source IDs and labels remain exact.
- A path-derived file UID is replaced with the SHA1 UID for that canonical path.
  Continued-session identities are compared verbatim, as are session/root/parent
  IDs and all uses of those identities in lists and detail maps.
- Top-level `generated_at` and `period.end_at` for `all` become
  `<GENERATED_AT>` after checking an ISO-like nonempty value. Historical event
  timestamps and bounded period dates are exact.
- Top-level `snapshot_token` must be nonempty with at least 16 characters, then
  becomes `<REVISION>`. HTTP tests separately check 204, identity binding and
  stale-token rejection.
- `parse_file` results omit explicitly private underscore-prefixed parser state;
  published scan results are compared in their entirety.

Timezone is UTC for this fixture corpus. Other timezones, DST transitions and
local midnight remain separate migration acceptance tests. File mtimes are
restored exactly; no `updated_at` fields are omitted.

## Existing HTTP and snapshot interfaces

The classic-only `/api/sessions` and `/api/session` routes have been removed.
`/api/daily-usage`, `/api/remotes`,
`/api/remotes/import`, `/api/remotes/rename`, `/api/remotes/delete` and
`/api/export.json` paths retain their original response shapes. A matching list
snapshot token returns 204 without a body. Detail with an unpublished token
returns 409 `{error: "snapshot stale", code: "snapshot_stale"}`. Missing detail
is 404. The complete event timeline remains in `/api/v2/chart` and snapshot exports.

Export keeps `schema: "cousash.remote-snapshot"`, `version: 1`, device identity,
and `snapshot` summaries/details/daily usage. The suggested export filename uses
the `cud-` prefix; importing snapshots does not depend on their filename.
Remote import retains its explicit confirmation behavior for same-device import
and missing display name. Remote deletion requires `confirm: true`. Imported
snapshots remain user data outside the installation and rebuildable cache.

Storage remains `COUSASH_CONFIG_DIR` when set; otherwise macOS
`~/Library/Application Support/cousash`, Windows `%APPDATA%/cousash` (with the
existing home fallback), or Linux `$XDG_CONFIG_HOME/cousash` / `~/.config/cousash`.
`remotes/` and device seed are preserved. The parse cache remains independently
versioned. Device identity algorithm is unchanged.

## Additive query interfaces (stage 1)

All `/api/v2/*` queries below require `id` and `snapshot_token`; they read a
published immutable snapshot and do not trigger a scan. Unknown revision is 409,
unknown session 404, malformed query 400. This makes revision, source identity
and date range inherited from the caller's list result.

- `/api/v2/session`: legacy detail without `timeline`, plus `revision`.
- `/api/v2/events`: `revision`, `id`, `events`, `offset`, `total`, `limit`,
  `next_cursor`, `previous_cursor`. Default/max limit 100. Cursors bind revision,
  session and page size. Events sort descending by parsed timestamp with stable
  input-order ties; invalid timestamps sort last. No deduplication occurs.
- `/api/v2/chart`: `revision`, `id`, and the full ordered `timeline`, projected to
  timestamp and cumulative total tokens. No downsampling is performed.

Health adds `protocol_version`, `implementation`, `source_sha256` and `build_id`.
The hash is captured when the handler is created, so editing the file without
restarting cannot make the old process claim to run the new source.

## Verification

```bash
python3 -m unittest discover -s tests -v
python3 tools/compatibility.py
npm run validate
# Existing benchmark dependencies / synthetic payload generator:
npm ci --prefix benchmarks/refactor/frontend
python3 benchmarks/refactor/backend.py generate
python3 tools/verify_stage1.py
```

The real-browser verification uses Chromium from the local Playwright cache,
port 8765 and isolated synthetic logs/config. It restores the worktree dashboard
with default data sources in `finally`. It compares the frozen legacy Git source
and current source in the same Chromium process. Raw new evidence is written to
`benchmarks/implementation/results/stage1.json`, not over the original baseline.

## Rust host and Svelte query contract

The preceding detail-only v2 description applies to session/event/chart routes.
The Rust host additionally provides:

| Route | Publication and paging rules |
| --- | --- |
| `/api/v2/sessions` | Full-filter metrics plus at most 100 summaries; optional revision-bound cursor. |
| `/api/v2/projects` | Project totals for the full filter, then paginated project rows. |
| `/api/v2/project-tasks` | Requires a published revision and project key; one task group retains its main/child identity across page boundaries. |
| `/api/revisions` | SSE revision/progress only. Initial connection and reconnect receive current status. |
| `/api/shutdown` | POST restricted by a per-launch private bearer token; used by the compatibility launcher. No token is exposed in health. |

List filters are search/model/source/environment/agents/sort/ascending plus the
period, custom dates and named timezone. Query cursors bind the revision, filter,
query kind and limit. Metrics cover the full filtered set, not the visible page.
`if_query_token` returns 204 only for the same query, page offset and revision.
`offset` can re-anchor a new publication at the previous page, clamped to the last
available page; returned cursors still bind the new publication. Event pages and
project-task pages support the same refresh behavior. Retained list revisions
return their corresponding source/remote metadata as well as session data.

The server retains up to eight views, rather than promising indefinite historical
access. A retired token returns 409. Details, events and charts must use the same
view token. The HTTP adapter cancels superseded requests and rejects late responses
even when the underlying transport ignores cancellation. A selected row can remain
selected after filtering; refresh keeps its detail page when it still exists.

Legacy `/api/sessions`, `/api/session`, `/api/daily-usage`, `/api/remotes` and
remote POST routes continue to use their old response contracts. `/api/export.json`
and CLI export are local-device-only v1 snapshots; `--once` includes imported
remote sessions as before. Sparse older remote details retain missing/null/extra
fields on legacy routes, including period slicing. Summary-only archives remain
visible in the all-period list and return 404 for unavailable details.

Health adds PID, configured Codex homes and, for managed Rust launches, a random
instance ID. `implementation=rust`, `ui=svelte`, protocol version 1 and capability
flags identify the host. The Svelte UI is served at `/` and `/index.html`;
`/legacy` returns 404 and old `/svelte` bookmarks redirect to `/` with their query intact.
Rust build ID hashes core/server sources, manifests, pricing assets and Svelte sources/build assets. The binary's
`--build-info` is compared against both health and the install manifest.
Python is no longer a supported runtime. Optional differential tests load the
frozen reference from Git history; all installed commands execute Rust.

Native file identity uses Unix device/inode/ctime or Windows volume/file ID plus
ChangeTime. The latter is separately exposed by the Windows file information API,
not equivalent to modification time; see [Microsoft FILE_BASIC_INFO](https://learn.microsoft.com/windows/win32/api/winbase/ns-winbase-file_basic_info).
Inventory I/O errors retain the last published snapshot and surface an index error;
missing session directories on a new installation are allowed. Log contents are
never rewritten. The background writer and remote store share one publication
cycle; queries do not initiate index reconstruction.

Old price segments can lack the later-added `service_tier` and `cost_multiplier`.
The typed boundary uses standard-tier defaults internally while legacy field
shapes retain the original omissions. `remote-old-price-segments.json` verifies
stored archives, all-period details and ranged repricing against Python.

The watcher observes Codex-home auxiliary files non-recursively and only recurses
into `sessions/` and `archived_sessions/`; plugin caches and worktrees are excluded.
New/replaced log roots are registered during polling. This keeps indexing ownership
in the backend without subscribing to unrelated package or workspace trees.
