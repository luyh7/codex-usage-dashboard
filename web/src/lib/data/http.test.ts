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
