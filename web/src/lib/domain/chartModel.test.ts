import { test } from 'node:test';
import assert from 'node:assert/strict';
import { chartDates, curveSegments, usageBars, type Point } from './chartModel.ts';

const now = Date.parse('2026-10-22T04:00:00Z');
test('date labels distinguish today, historical days, midnight and multiple days in the display timezone', () => {
  const stamps = [
    '2026-10-21T12:17:00Z',
    '2026-10-21T13:45:00Z',
    '2026-10-21T15:14:00Z',
    '2026-10-21T16:42:00Z',
  ];
  const shanghai = chartDates('Asia/Shanghai', stamps, now);
  assert.deepEqual(shanghai.ticks(stamps), [
    { time: '20:17', date: '10/21' },
    { time: '21:45', date: '' },
    { time: '23:14', date: '' },
    { time: '00:42', date: '10/22' },
  ]);
  assert.equal(shanghai.tooltip(stamps[3]), '10/22 00:42');
  assert.deepEqual(
    chartDates('UTC', stamps, now)
      .ticks(stamps)
      .map((t) => t.date),
    ['10/21', '', '', ''],
  );
  const today = ['2026-10-22T01:00:00Z', '2026-10-22T02:00:00Z'];
  assert.deepEqual(
    chartDates('Asia/Shanghai', today, now)
      .ticks(today)
      .map((t) => t.date),
    ['', ''],
  );
  assert.equal(chartDates('Asia/Shanghai', today, now).tooltip(today[0]), '10/22 09:00');
  const days = ['2026-10-19T01:00:00Z', '2026-10-20T01:00:00Z', '2026-10-22T01:00:00Z'];
  assert.deepEqual(
    chartDates('UTC', days, now).ticks(days),
    days.map((stamp) => ({ time: '', date: stamp.slice(5, 10).replace('-', '/') })),
  );
});

test('years and missing or invalid timestamps remain unambiguous', () => {
  const stamps = ['2025-12-31T15:00:00Z', '2025-12-31T17:00:00Z', null, 'invalid'];
  const dates = chartDates('Asia/Shanghai', stamps, now);
  assert.equal(dates.tooltip(stamps[0]), '2025/12/31 23:00');
  assert.equal(dates.tooltip(stamps[1]), '2026/01/01 01:00');
  assert.equal(dates.tooltip(null), null);
  assert.equal(dates.tooltip('invalid'), null);
  assert.deepEqual(dates.ticks(stamps).slice(2), [
    { time: '#3', date: '' },
    { time: '#4', date: '' },
  ]);
});

test('cubic segments retain real values with uneven or repeated timestamps', () => {
  const points: Point[] = [
    [0, 100],
    [0, 90],
    [1, 80],
    [100, 79],
    [101, 79],
    [102, 0],
    [102, 20],
    [300, 30],
  ];
  const segments = curveSegments(points);
  assert.equal(segments.length, points.length - 1);
  segments.forEach(([x1, y1, x2, y2, x3, y3], i) => {
    const [x0, y0] = points[i];
    assert.ok([x1, y1, x2, y2, x3, y3].every(Number.isFinite));
    for (let j = 0; j <= 100; j++) {
      const t = j / 100,
        a = 1 - t;
      const y = a ** 3 * y0 + 3 * a ** 2 * t * y1 + 3 * a * t ** 2 * y2 + t ** 3 * y3;
      assert.ok(y >= Math.min(y0, y3) - 1e-8 && y <= Math.max(y0, y3) + 1e-8);
    }
    assert.ok(x0 <= x1 && x1 <= x2 && x2 <= x3);
  });
  assert.deepEqual(curveSegments([]), []);
  assert.deepEqual(curveSegments([[0, 0]]), []);
});

test('columns preserve token deltas, zero intervals and dense bursts', () => {
  const timeline = [20, 30, 30, 80, 100].map((total_tokens) => ({
    timestamp: null,
    total_token_usage: { total_tokens },
  }));
  const { bars } = usageBars(timeline, [0, 0.001, 0.1, 0.99, 1], 100);
  assert.equal(
    bars.reduce((sum, bar) => sum + bar.tokens, 0),
    100,
  );
  assert.deepEqual(bars, [
    { column: 0, start: 0, end: 1, tokens: 30 },
    { column: 1, start: 2, end: 2, tokens: 0 },
    { column: 9, start: 3, end: 4, tokens: 70 },
  ]);
  assert.equal(usageBars(timeline, [0, 0.001, 0.1, 0.99, 1], 1).bars[0].tokens, 100);
  assert.deepEqual(usageBars([], [], 100).bars, []);
});
