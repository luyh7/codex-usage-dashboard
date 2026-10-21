import { test } from 'node:test';
import assert from 'node:assert/strict';
import { costSegments, chartPositions, displayMoney } from './presentation.ts';

test('cost composition includes cache writes but never counts reasoning output twice', () => {
  const parts = costSegments({
    estimated_cost_breakdown_usd: {
      input_tokens: 2.1,
      cached_input_tokens: 1.26,
      output_tokens: 3.12,
      reasoning_output_tokens: 2,
      cache_write_tokens: 0.5,
    },
  });
  assert.equal(parts.length, 4);
  assert.equal(
    parts.reduce((sum, part) => sum + part.value, 0),
    6.98,
  );
  assert.deepEqual(costSegments({ estimated_cost_breakdown_usd: null }), []);
  assert.equal(costSegments({ estimated_cost_breakdown_usd: { output_tokens: 0 } }).length, 3);
});

test('sparse timestamps retain elapsed-time spacing; incomplete imports use event order', () => {
  const rows = ['2026-10-21T09:00:00Z', '2026-10-21T09:01:00Z', '2026-10-21T09:10:00Z'].map(
    (timestamp, i) => ({ timestamp, total_token_usage: { total_tokens: i * 100 } }),
  );
  assert.deepEqual(chartPositions(rows), [0, 0.1, 1]);
  assert.deepEqual(
    chartPositions(rows.map((r, i) => ({ ...r, timestamp: i === 1 ? null : r.timestamp }))),
    [0, 0.5, 1],
  );
  assert.deepEqual(chartPositions([rows[0]]), [0]);
  assert.deepEqual(chartPositions([]), []);
});

test('small nonzero estimates stay visible instead of appearing free', () => {
  assert.equal(displayMoney(0.000123), '$0.000123');
  assert.equal(displayMoney(18.623456), '$18.62');
  assert.equal(displayMoney(0), '$0.00');
});
