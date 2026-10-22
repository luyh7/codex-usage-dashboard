import test from 'node:test';
import assert from 'node:assert/strict';
import { modelColor, modelRows } from './modelBreakdown';
import type { ModelBreakdown, Usage } from '../data/types';
const row = (model: string, tokens: number, cost: number | null): ModelBreakdown => ({
  model, usage: { total_tokens: tokens } as Usage, estimated_cost_usd: cost, cost_complete: cost !== null,
});
test('model shares sort by selected measure, retain unknowns and use stable colors across filters', () => {
  const source = [row('gpt-6.1-sol', 800, 1), row('gpt-6-astra', 200, 3), row('unknown', 100, null)];
  const tokens = modelRows(source, 'tokens'), costs = modelRows(source, 'cost');
  assert.deepEqual(tokens.map(r => r.model), ['gpt-6.1-sol', 'gpt-6-astra', 'unknown']);
  assert.deepEqual(costs.map(r => r.model), ['gpt-6-astra', 'gpt-6.1-sol', 'unknown']);
  assert.equal(costs[0].share, .75);
  assert.equal(tokens[0].share, 800 / 1100);
  assert.equal(costs[2].share, 0);
  assert.equal(modelRows([source[1]], 'tokens')[0].color, tokens[1].color);
  assert.notEqual(modelColor('gpt-6-sol'), modelColor('gpt-6-luna'));
  assert.equal(source[0].model, 'gpt-6.1-sol');
});
test('zero totals and unpriced models never invent proportional costs', () => {
  assert.deepEqual(modelRows([], 'tokens'), []);
  assert.equal(modelRows([row('free', 0, 0)], 'cost')[0].share, 0);
  assert.equal(modelRows([row('unpriced', 100, null)], 'cost')[0].estimated_cost_usd, null);
});
