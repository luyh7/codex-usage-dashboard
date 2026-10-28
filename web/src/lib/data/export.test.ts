import { test } from 'node:test';
import assert from 'node:assert/strict';
import { downloadSnapshot } from '../../../shared/snapshot-export.js';

test('redacted mode stays attached to job creation and download', async () => {
  const calls: string[] = [];
  await downloadSnapshot('/api/export.json.gz?mode=redacted', () => {}, {
    fetcher: (async input => {
      const url = String(input); calls.push(url);
      if (url.endsWith('/security/token')) return Response.json({token:'test'});
      if (url.includes('/start')) return Response.json({id:'shared-job',phase:'ready',percent:95});
      return new Response(new Uint8Array([1]));
    }) as typeof fetch,
    save: (_blob, filename) => assert.equal(filename, 'cud-shared.json.gz'),
  });
  assert(calls.includes('/api/export/start?mode=redacted'));
  assert(calls.includes('/api/export.json.gz?job=shared-job&mode=redacted'));
});

test('download preserves Unicode device names and falls back for malformed headers', async () => {
  for (const [header, expected] of [
    ["attachment; filename=\"cud-mac-123.json.gz\"; filename*=UTF-8''cud-%E7%A0%94%E5%8F%91-mac-123.json.gz", 'cud-研发-mac-123.json.gz'],
    ["attachment; filename=\"fallback.json.gz\"; filename*=UTF-8''%ZZ", 'fallback.json.gz'],
  ]) {
    await downloadSnapshot('/api/export.json.gz', () => {}, {
      fetcher: (async input => {
        if (String(input).endsWith('/security/token')) return Response.json({ token: 'test' });
        if (String(input).endsWith('/start')) return Response.json({ id: 'job', phase: 'ready', percent: 95 });
        return new Response(new Uint8Array([1]), { headers: { 'content-disposition': header } });
      }) as typeof fetch,
      save: (_blob, filename) => assert.equal(filename, expected),
    });
  }
});

test('export follows server work and downloaded bytes monotonically before saving', async () => {
  const states = [
    { id: 'job', phase: 'preparing', percent: 1 },
    { id: 'job', phase: 'compressing', percent: 42 },
    { id: 'job', phase: 'compressing', percent: 40 },
    { id: 'job', phase: 'ready', percent: 95 },
  ];
  const progress: number[] = [];
  let saved: Blob | undefined;
  await downloadSnapshot('/api/export.json.gz', value => progress.push(value), {
    pause: async () => {},
    fetcher: (async (input, init) => {
      if (String(input).endsWith('/api/security/token')) return Response.json({ token: 'fixture-token' });
      if (String(input).includes('.json.gz?job=')) {
        assert.equal(states.length, 0);
        const stream = new ReadableStream({ start(controller) {
          controller.enqueue(new Uint8Array([1, 2]));
          controller.enqueue(new Uint8Array([3, 4, 5, 6])); controller.close();
        } });
        return new Response(stream, { headers: { 'content-length': '6', 'content-disposition': 'attachment; filename="test.json.gz"' } });
      }
      if (String(input).endsWith('/start')) {
        assert.equal(init?.method, 'POST');
        assert.equal(new Headers(init?.headers).get('X-Codex-Write-Token'), 'fixture-token');
        assert.equal(new Headers(init?.headers).get('Content-Type'), 'application/json');
      }
      return Response.json(states.shift());
    }) as typeof fetch,
    save: (blob, filename) => { assert.equal(filename, 'test.json.gz'); assert(progress.at(-1)! < 100); saved = blob; },
  });
  assert.deepEqual(progress, [0, 1, 42, 42, 95, 96, 99, 100]);
  assert.deepEqual([...new Uint8Array(await saved!.arrayBuffer())], [1, 2, 3, 4, 5, 6]);
});

test('failed generation and incomplete downloads never report success or save', async () => {
  for (const failure of ['generation', 'download']) {
    const progress: number[] = [];
    await assert.rejects(downloadSnapshot('/api/export.json.gz', value => progress.push(value), {
      fetcher: (async input => {
        if (String(input).endsWith('/api/security/token')) return Response.json({ token: 'fixture-token' });
        if (String(input).endsWith('/start')) return Response.json(failure === 'generation'
          ? { phase: 'error', error: 'Export failed' } : { id: 'job', phase: 'ready', percent: 95 });
        return new Response(new Uint8Array([1]), { headers: { 'content-length': '2' } });
      }) as typeof fetch,
      save: () => assert.fail('An unsuccessful export must not be saved'),
    }));
    assert(!progress.includes(100));
  }
});
