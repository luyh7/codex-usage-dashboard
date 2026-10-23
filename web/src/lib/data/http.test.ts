import { test } from 'node:test';
import assert from 'node:assert/strict';
import { HttpDashboard, DataError, cancelled } from './http.ts';
import type { Query } from './types.ts';
const query: Query = {
  period: 'all',
  start: '',
  end: '',
  timezone: 'UTC',
  search: '',
  model: '',
  source: '',
  environment: '',
  agents: '',
  sort: 'tokens',
  ascending: false,
};
test('detail validates the full filter before loading events and preserves error codes', async () => {
  const paths: string[] = [];
  const data = new HttpDashboard('', (async input => {
    const url = new URL(String(input), 'http://localhost');
    paths.push(url.pathname);
    assert.equal(url.searchParams.get('check_filters'), 'true');
    assert.equal(url.searchParams.get('source'), 'archived');
    assert.equal(url.searchParams.get('snapshot_token'), 'r1');
    return Response.json({ error: 'excluded', code: 'session_filtered_out' }, { status: 404 });
  }) as typeof fetch);
  await assert.rejects(data.detail('one', 'r1', { ...query, source: 'archived' }),
    (error: unknown) => error instanceof DataError && error.code === 'session_filtered_out');
  assert.deepEqual(paths, ['/api/v2/session']);
});
test('filter changes cancel a late detail failure without starting old event requests', async () => {
  let finish!: (response: Response) => void;
  const paths: string[] = [];
  const data = new HttpDashboard('', ((input: string) => {
    paths.push(new URL(input, 'http://localhost').pathname);
    return new Promise<Response>(resolve => { finish = resolve; });
  }) as typeof fetch);
  const pending = data.detail('one', 'r1', query).catch(error => error);
  data.cancelDetails();
  finish(Response.json({ error: 'missing', code: 'details_missing' }, { status: 404 }));
  assert(cancelled(await pending));
  assert.deepEqual(paths, ['/api/v2/session']);
});
test('late transport responses cannot overwrite a newer filter, even if abort is ignored', async () => {
  const pending: ((response: Response) => void)[] = [];
  const transport = (() =>
    new Promise<Response>((resolve) => pending.push(resolve))) as typeof fetch;
  const data = new HttpDashboard('', transport);
  const old = data.projects(query).catch((error) => error);
  const fresh = data.projects({ ...query, search: 'new' });
  pending[1](Response.json({ page: { revision: 'r2' } }));
  assert.equal((await fresh)?.page.revision, 'r2');
  pending[0](Response.json({ page: { revision: 'r1' } }));
  assert(cancelled(await old));
});
test('unchanged responses do not replace data; revision mismatch is explicit', async () => {
  const data = new HttpDashboard(
    '',
    (async () => new Response(null, { status: 204 })) as typeof fetch,
  );
  assert.equal(await data.projects(query), null);
  const wrong = new HttpDashboard('', (async () =>
    Response.json({ revision: 'other', id: 'one' })) as typeof fetch);
  await assert.rejects(
    wrong.events('one', 'expected', ''),
    (error: unknown) => error instanceof DataError && error.stale,
  );
});

test('snapshot files upload unchanged with encoded labels and confirmation on retry', async () => {
  const file = new Blob([new Uint8Array([0x1f, 0x8b, 0x08])], { type: 'application/gzip' });
  let calls = 0;
  const data = new HttpDashboard('', (async (input, init) => {
    const url = new URL(String(input), 'http://localhost');
    assert.equal(url.pathname, '/api/remotes/import-file');
    assert.equal(init?.body, file);
    assert.equal(init?.method, 'POST');
    if (calls++ === 0) {
      assert.equal(url.searchParams.get('allow_current_device'), 'false');
      return Response.json({ needs_confirmation: true }, { status: 409 });
    }
    assert.equal(url.searchParams.get('allow_current_device'), 'true');
    assert.equal(url.searchParams.get('label'), '设备 A & B + 备份');
    return Response.json({ ok: true });
  }) as typeof fetch);
  assert.equal((await data.importSnapshot(file)).needs_confirmation, true);
  assert.equal((await data.importSnapshot(file, '设备 A & B + 备份', true)).ok, true);
  assert.equal(calls, 2);
  assert.equal(data.exportUrl(), '/api/export.json.gz');
});

test('invalid snapshot file errors surface and legacy object imports stay compatible', async () => {
  const broken = new HttpDashboard('', (async () =>
    Response.json({ error: 'invalid gzip checksum' }, { status: 400 })) as typeof fetch);
  await assert.rejects(broken.importSnapshot(new Blob(['broken'])), /invalid gzip checksum/);
  const snapshot = { schema: 'cousash.remote-snapshot', version: 1 };
  const legacy = new HttpDashboard('', (async (input, init) => {
    assert.equal(input, '/api/remotes/import');
    assert.deepEqual(JSON.parse(String(init?.body)), { snapshot, label: 'Legacy', allow_current_device: false });
    return Response.json({ ok: true });
  }) as typeof fetch);
  assert.equal((await legacy.importSnapshot(snapshot, 'Legacy')).ok, true);
});
