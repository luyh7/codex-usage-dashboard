import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  costSegments,
  chartPositions,
  displayMoney,
  relativeTime,
  activityTime,
  latestActivityTime,
  sessionMetadata,
  serviceTier,
  cacheHitPercent,
  cacheHitTone,
} from './presentation.ts';
import { defaultQuery } from './ui.ts';

test('session metadata preserves multiple models and efforts and uses the scoped aggregate cache ratio', () => {
  const sessions = [
    { model: 'model-a', models: ['model-a', 'model-b'], effort: ' high ' },
    { model: 'model-b', models: [], effort: 'low' },
    { model: '', models: ['model-a'], effort: 'high' },
  ];
  assert.deepEqual(sessionMetadata(sessions), {
    models: ['model-a', 'model-b'], efforts: ['high', 'low'], speedTiers: [],
  });
  assert.deepEqual(sessionMetadata([{ model: '', models: [], effort: '' }]), {
    models: [], efforts: [], speedTiers: [],
  });
  assert.equal(cacheHitPercent({ input_tokens: 1000, cached_input_tokens: 810 }), 81);
  assert.equal(cacheHitPercent({ input_tokens: 0, cached_input_tokens: 0 }), null);
  assert.equal(cacheHitPercent({ input_tokens: 300, cached_input_tokens: 100 }), 33.3);
});

test('speed metadata normalizes Fast aliases and uses current tiers rather than historical modes', () => {
  const base = { model: 'model-a', models: [], effort: 'high' };
  assert.deepEqual(sessionMetadata([
    { ...base, service_tier: ' Priority ' },
    { ...base, service_tier: 'FAST' },
    { ...base, service_tier: 'ultrafast' },
    { ...base, service_tier: 'standard' },
  ]).speedTiers, ['fast', 'ultrafast']);
  const switched = { ...base, service_tier: 'standard', service_tiers: ['priority', 'standard'] };
  assert.deepEqual(sessionMetadata([switched]).speedTiers, []);
  for (const tier of ['', undefined, null]) assert.equal(serviceTier(tier), 'standard');
  assert.equal(serviceTier(' custom '), 'custom');
  assert.deepEqual(sessionMetadata([{ ...base, service_tier: 'custom' }]).speedTiers, []);
});

test('cache colors share exact display boundaries and keep missing data neutral', () => {
  const cases = [
    [0, 'low'], [49.9, 'low'], [50, 'medium'], [79.9, 'medium'],
    [80, 'high'], [94.9, 'high'], [95, 'excellent'], [100, 'excellent'],
    [null, 'empty'], [NaN, 'empty'],
  ] as const;
  for (const [value, tone] of cases) assert.equal(cacheHitTone(value), tone);
  assert.equal(cacheHitPercent({ input_tokens: 100, cached_input_tokens: 120 }), 100);
  assert.equal(cacheHitPercent({ input_tokens: 100, cached_input_tokens: -1 }), 0);
  assert.equal(cacheHitPercent({ input_tokens: 100, cached_input_tokens: NaN }), null);
  assert.equal(cacheHitTone(cacheHitPercent({ input_tokens: 10000, cached_input_tokens: 4999 })), 'medium');
});

test('overview defaults to recent activity and timestamps match that activity across timezones', () => {
  assert.equal(defaultQuery().sort, 'updated');
  assert.equal(defaultQuery().ascending, false);
  const row = {
    last_usage_at: '2026-10-22T12:00:00+08:00',
    end_at: '2026-10-22T12:00:00+08:00',
    updated_at: '2026-10-23T00:00:00Z',
    start_at: '',
    created_at: '',
  };
  assert.equal(activityTime(row), row.last_usage_at);
  assert.equal(activityTime({ ...row, last_usage_at: null }), null);
  assert.equal(activityTime({ ...row, last_usage_at: undefined }), undefined);
  assert.equal(latestActivityTime([row, { last_usage_at: '2026-10-22T05:00:00Z' }]), '2026-10-22T05:00:00Z');
  assert.equal(latestActivityTime([{ last_usage_at: null }, { last_usage_at: 'invalid' }]), undefined);
  assert.equal(relativeTime(activityTime(row), 'zh', Date.parse('2026-10-22T04:07:00Z')), '7 分钟');
  assert.equal(
    relativeTime('2026-10-22T04:00:00Z', 'en', Date.parse('2026-10-22T06:00:00Z')),
    '2 h',
  );
  assert.equal(
    relativeTime('2026-10-23T04:00:00Z', 'zh', Date.parse('2026-10-22T04:00:00Z')),
    '刚刚',
  );
  assert.equal(relativeTime('invalid', 'zh'), '—');
  assert.equal(relativeTime(undefined, 'en'), '—');
});

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
