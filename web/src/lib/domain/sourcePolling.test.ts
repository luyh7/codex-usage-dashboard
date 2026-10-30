import assert from 'node:assert/strict';
import { test } from 'node:test';
import { clampPollInterval, networkPollingLabel, withSourcePolling } from './sourcePolling.ts';

test('network poll intervals are clamped to at least 10 seconds', () => {
  assert.equal(clampPollInterval(3), 10);
  assert.equal(clampPollInterval(0), 10);
  assert.equal(clampPollInterval(10), 10);
  assert.equal(clampPollInterval(45.4), 45);
  assert.equal(clampPollInterval(Number.NaN), 30);
});

test('source polling patches keep other sources and drop cleared entries', () => {
  const current = { '/a': { poll_interval_secs: 60 }, '/b': { poll_interval_secs: 20 } };
  assert.deepEqual(withSourcePolling(current, '/a', { poll_interval_secs: 5 }), { '/a': { poll_interval_secs: 10 }, '/b': { poll_interval_secs: 20 } });
  assert.deepEqual(withSourcePolling(current, '/b', { poll_interval_secs: undefined }), { '/a': { poll_interval_secs: 60 } });
  assert.deepEqual(withSourcePolling({}, '/c', { poll_interval_secs: 45 }), { '/c': { poll_interval_secs: 45 } });
});

test('network polling chips say what the interval does and default to 30 seconds', () => {
  assert.equal(networkPollingLabel(undefined, 'zh'), '网络盘 · 每 30 秒检查');
  assert.equal(networkPollingLabel(45, 'zh'), '网络盘 · 每 45 秒检查');
  assert.equal(networkPollingLabel(undefined, 'en'), 'Network drive · checks every 30 s');
});
