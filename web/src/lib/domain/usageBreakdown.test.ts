import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { Detail } from '../data/types';
import { segmentPriceTooltip, usageBreakdown, usagePriceTooltip } from './usageBreakdown';

const detail = {
  effort: 'high',
  total_token_usage: { input_tokens: 1000, cached_input_tokens: 800, output_tokens: 100, reasoning_output_tokens: 60, cache_write_tokens: 0 },
  estimated_cost_breakdown_usd: { input_tokens: .002, cached_input_tokens: .0008, output_tokens: .005, reasoning_output_tokens: .003 },
  applied_price_segments: [{ model: 'example', service_tier: 'ultrafast', cost_multiplier: 6, context_tier: 'long', prices: { input: 10, cached_input: 1, output: 50 }, usage: { input_tokens: 1000, cached_input_tokens: 800, output_tokens: 100, reasoning_output_tokens: 60 } }],
} as unknown as Detail;

test('breakdown subtracts cached input and preserves reasoning as part of output', () => {
  assert.deepEqual(usageBreakdown(detail).map(part => [part.key, part.tokens]), [
    ['input', 200], ['cached', 800], ['output', 100], ['reasoning', 60],
  ]);
  const parts = usageBreakdown({ ...detail, total_token_usage: { ...detail.total_token_usage, cache_write_tokens: 12 } });
  assert.equal(parts.find(part => part.key === 'cacheWrite')?.tokens, 12);
  assert.equal(usageBreakdown({ ...detail, estimated_cost_breakdown_usd: null }).every(part => part.cost === null), true);
});

test('one price hint includes every category, exact usage and recorded rates without multiplying twice', () => {
  const hint = usagePriceTooltip(detail, 'zh');
  const table = hint.tables![0];
  assert.equal(hint.tables!.length, 1);
  assert.ok(table.rows.some(row => row.label === '输入' && row.values[0] === '200'));
  for (const label of ['缓存输入', '输出', '其中推理']) assert.ok(table.rows.some(row => row.label === label));
  const rates = table.rows.map(row => row.values[1]);
  for (const rate of ['$10.00', '$1.00', '$50.00']) assert.ok(rates.includes(rate));
  assert.ok(!rates.includes('$60.00'));
  assert.equal(table.caption, 'example · high · Ultrafast · 长上下文');
  assert.equal(table.modelTags?.longContext, true);
  assert.equal(hint.note, '推理费用已包含在输出中。');
  const zeroRate = structuredClone(detail);
  zeroRate.applied_price_segments[0].prices = { input: 0 };
  assert.equal(usagePriceTooltip(zeroRate, 'en').tables![0].rows[0].values[1], '$0.00');
  assert.equal(usagePriceTooltip({ ...detail, estimated_cost_breakdown_usd: null, applied_price_segments: [] }, 'en').tables![0].rows[0].values[2], 'Unknown');
});

test('identical price segments merge while distinct models and rates retain separate table headings', () => {
  const next = structuredClone(detail);
  next.applied_price_segments.push(structuredClone(next.applied_price_segments[0]));
  const other = structuredClone(next.applied_price_segments[0]);
  other.model = 'second-model';
  next.applied_price_segments.push(other);
  const tables = usagePriceTooltip(next, 'en').tables!;
  assert.equal(tables.length, 2);
  assert.equal(tables[0].rows[0].values[0], '400');
  assert.equal(tables[1].rows[0].values[0], '200');
  assert.equal(tables[0].rows[0].values[2], '$0.004');
  assert.equal(tables[1].caption, 'second-model · high · Ultrafast · Long context');
});

test('price tags normalize Fast aliases, hide short context, and preserve effort/context groups', () => {
  const next = structuredClone(detail);
  const segment = { ...next.applied_price_segments[0], service_tier: 'priority', context_tier: 'short' };
  next.applied_price_segments = [segment, { ...segment, service_tier: 'fast' },
    { ...segment, context_tier: 'long' }, { ...segment, effort: 'medium' }];
  const tables = usagePriceTooltip(next, 'zh').tables!;
  assert.equal(tables.length, 3);
  assert.equal(tables[0].caption, 'example · high · Fast');
  assert.equal(tables[0].modelTags?.longContext, false);
  assert.equal(tables[0].rows[0].values[0], '400');
  assert.equal(tables[1].modelTags?.longContext, true);
  assert.equal(tables[2].modelTags?.effort, 'medium');
});

test('segment price hints use only that segment usage and costs, never session totals', () => {
  const segment = {
    ...detail.applied_price_segments[0], context_tier: 'short', effort: 'medium',
    usage: { ...detail.total_token_usage, input_tokens: 200, cached_input_tokens: 80, output_tokens: 50, reasoning_output_tokens: 20 },
  };
  const table = segmentPriceTooltip(detail, segment, 'en').tables![0];
  assert.equal(table.rows[0].values[0], '120');
  assert.equal(table.rows[0].values[2], '$0.0012');
  assert.equal(table.rows[2].values[0], '50');
  assert.equal(table.modelTags?.effort, 'medium');
  assert.equal(table.modelTags?.longContext, false);
  const recorded = { ...segment, estimated_cost_breakdown_usd: { input_tokens: .001199 } };
  assert.equal(segmentPriceTooltip(detail, recorded, 'en').tables![0].rows[0].values[2], '$0.001199');
});

test('historical prices display schedule validity in the selected timezone and merge Standard aliases', () => {
  const old = { ...detail.applied_price_segments[0], service_tier: '',
    effective_at: '2026-07-09T17:00:00Z', effective_until: '2026-08-21T00:00:00Z' };
  const current = { ...old, service_tier: 'default', effective_at: '2026-08-21T00:00:00Z', effective_until: null,
    prices: { input: 4, cached_input: .4, output: 20 } };
  const next = { ...detail, applied_price_segments: [old, { ...old, service_tier: 'default' }, current] };
  const tables = usagePriceTooltip(next, 'zh', 'Asia/Shanghai').tables!;
  assert.equal(tables.length, 2);
  assert.equal(tables[0].rows[0].values[0], '400');
  assert.equal(tables[0].period, '2026/07/10 01:00 – 2026/08/21 08:00');
  assert.equal(tables[1].period, '2026/08/21 08:00 起');
  assert.equal(usagePriceTooltip(next, 'en', 'UTC').tables![1].period, 'From 2026/08/21 00:00');
  for (const table of tables) assert.doesNotMatch(table.period!, /Asia\/Shanghai|UTC|GMT|[+]08/);
  assert.equal(segmentPriceTooltip(next, old, 'zh', 'Asia/Shanghai').tables![0].period, tables[0].period);
  const returning = { ...old, effective_at: '2026-10-01T00:00:00Z', effective_until: null };
  assert.equal(usagePriceTooltip({ ...next, applied_price_segments: [old, returning] }, 'en').tables!.length, 2);
  const undated = { ...old, effective_at: 'invalid', effective_until: null };
  assert.equal(usagePriceTooltip({ ...detail, applied_price_segments: [undated] }, 'en').tables![0].period, '');
});
