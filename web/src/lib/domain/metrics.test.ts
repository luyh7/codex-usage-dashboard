import { test } from 'node:test';
import assert from 'node:assert/strict';
import { metricSeries, metricAxisLabel, metricIntervalLabel } from './metrics.ts';
import { zero } from './ui.ts';

test('metric trends retain zero days, query boundaries and unknown pricing', () => {
  const points = [
    {
      date: '2026-10-19',
      usage: { ...zero(), total_tokens: 10 },
      session_count: 2,
      estimated_cost_usd: 1,
    },
    {
      date: '2026-10-21',
      usage: { ...zero(), total_tokens: 20 },
      session_count: 3,
      estimated_cost_usd: null,
    },
    {
      date: '2026-10-23',
      usage: { ...zero(), total_tokens: 50 },
      session_count: 1,
      estimated_cost_usd: 9,
    },
  ];
  const result = metricSeries(
    { complete: true, points },
    { key: 'custom', start_date: '2026-10-19', end_date: '2026-10-22', start_at: null, end_at: '' },
  );
  assert.deepEqual(
    result.map((row) => row.tokens),
    [10, 0, 20, 0],
  );
  assert.deepEqual(
    result.map((row) => row.cost),
    [1, 0, null, 0],
  );
  assert.equal(
    result.reduce((sum, row) => sum + row.sessions, 0),
    5,
  );
});

test('long ranges are bounded without losing usage or cost and empty history stays empty', () => {
  const result = metricSeries(
    {
      complete: true,
      points: [
        {
          date: '2025-01-01',
          usage: { ...zero(), total_tokens: 100 },
          session_count: 1,
          estimated_cost_usd: 2,
        },
        {
          date: '2026-10-22',
          usage: { ...zero(), total_tokens: 200 },
          session_count: 2,
          estimated_cost_usd: 3,
        },
      ],
    },
    null,
  );
  assert.ok(result.length <= 28);
  assert.equal(result[0].date, '2025-01-01');
  assert.equal(result.at(-1)?.end, '2026-10-22');
  assert.equal(
    result.reduce((sum, row) => sum + row.tokens, 0),
    300,
  );
  assert.equal(
    result.reduce((sum, row) => sum + (row.cost ?? 0), 0),
    5,
  );
  assert.deepEqual(metricSeries(undefined, null), []);
  assert.deepEqual(metricSeries({ complete: true, points: [] }, null), []);
});

test('hourly series retain real intervals, zero hours and unknown prices', () => {
  const result = metricSeries({
    complete: true,
    granularity: 'hour',
    points: [0, 1, 2].map((hour) => ({
      date: `2026-10-22T0${hour}:00:00+08:00`,
      end: `2026-10-22T0${hour + 1}:00:00+08:00`,
      usage: { ...zero(), total_tokens: hour * 100 },
      session_count: hour,
      estimated_cost_usd: hour === 2 ? null : hour,
    })),
  }, null);
  assert.deepEqual(result.map((row) => row.tokens), [0, 100, 200]);
  assert.deepEqual(result.map((row) => row.cost), [0, 1, null]);
  assert.equal(metricAxisLabel(result[0].date), '00:00');
  assert.equal(metricAxisLabel('2026-10-22'), '10/22');
  assert.equal(metricIntervalLabel(result[1].date, result[1].end), '2026-10-22 01:00 – 02:00');
  assert.equal(metricIntervalLabel('2026-10-22T23:00:00+08:00', '2026-10-23T00:00:00+08:00'), '2026-10-22 23:00 – 24:00');
  assert.equal(metricIntervalLabel('2026-10-19', '2026-10-22'), '2026-10-19 – 2026-10-22');
});
