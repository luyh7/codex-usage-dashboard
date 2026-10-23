import { test } from 'node:test';
import assert from 'node:assert/strict';
import { downloadSnapshot } from '../../../shared/snapshot-export.js';

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
      if (String(input).includes('.json.gz?job=')) {
        assert.equal(states.length, 0);
        const stream = new ReadableStream({ start(controller) {
          controller.enqueue(new Uint8Array([1, 2]));
          controller.enqueue(new Uint8Array([3, 4, 5, 6])); controller.close();
        } });
        return new Response(stream, { headers: { 'content-length': '6', 'content-disposition': 'attachment; filename="test.json.gz"' } });
      }
      if (String(input).endsWith('/start')) assert.equal(init?.method, 'POST');
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
        if (String(input).endsWith('/start')) return Response.json(failure === 'generation'
          ? { phase: 'error', error: 'Export failed' } : { id: 'job', phase: 'ready', percent: 95 });
        return new Response(new Uint8Array([1]), { headers: { 'content-length': '2' } });
      }) as typeof fetch,
      save: () => assert.fail('An unsuccessful export must not be saved'),
    }));
    assert(!progress.includes(100));
  }
});
