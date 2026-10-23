import test from 'node:test';
import assert from 'node:assert/strict';
import { estimatedTps, formatTps, speedHint, eventSpeed, usageSpeedHint } from './speed';
import type { TokenEvent } from '../data/types';

test('TPS uses weighted totals, preserves small nonzero speeds, and marks log estimates', () => {
  const speed = { output_tokens: 400, duration_ms: 7000, request_count: 2, first_token_request_count: 1 };
  assert.equal(estimatedTps(speed), 400 / 7);
  assert.equal(formatTps(speed, 'zh'), '≈57.1 tok/s');
  assert.match(speedHint(speed, 'zh'), /1 条有首 token 计时，1 条包含首 token 等待/);
  assert.equal(formatTps({ output_tokens: 1, duration_ms: 3600000, request_count: 1 }, 'en'), '≈0.00028 tok/s');
  for (const value of [undefined, null, { ...speed, output_tokens: 0 }, { ...speed, duration_ms: 0 }, { ...speed, output_tokens: Infinity }, { ...speed, request_count: 0 }]) {
    assert.equal(estimatedTps(value), null);
    assert.equal(formatTps(value, 'en'), '—');
  }
});

test('event TPS uses request deltas and matches timing eligibility without hiding short outputs', () => {
  const event = { last_token_usage: { output_tokens: 20 }, total_token_usage: { output_tokens: 100000 }, request_duration_ms: 4000, generation_duration_ms: 2000 } as TokenEvent;
  assert.equal(estimatedTps(eventSpeed(event)), 10);
  assert.equal(formatTps(eventSpeed(event), 'en', true), '10');
  assert.equal(usageSpeedHint(eventSpeed(event), 'en'), '10 tok/s');
  assert.equal(estimatedTps(eventSpeed({ ...event, generation_duration_ms: null })), 5);
  for (const change of [{ generation_duration_ms: 0 }, { generation_duration_ms: 99 }, { generation_duration_ms: 3600001 }, { generation_duration_ms: undefined, request_duration_ms: 999 }, { last_token_usage: null }]) {
    assert.equal(eventSpeed({ ...event, ...change }), null);
  }
});
