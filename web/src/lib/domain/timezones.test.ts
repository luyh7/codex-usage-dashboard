import { test } from 'node:test';
import assert from 'node:assert/strict';
import { timezoneOption } from './timezones.ts';

test('time zone labels localize cities while preserving IANA values and fractional offsets', () => {
  const now = new Date('2026-10-22T00:00:00Z');
  assert.deepEqual(timezoneOption('Asia/Shanghai', 'zh', now), ['Asia/Shanghai', '上海 (UTC+08:00)']);
  assert.equal(timezoneOption('Asia/Shanghai', 'en', now)[1], 'Shanghai (UTC+08:00)');
  assert.match(timezoneOption('Asia/Kathmandu', 'zh', now)[1], /加德满都 \(UTC\+05:45\)/);
  assert.equal(timezoneOption('UTC', 'zh', now)[1], '协调世界时 (UTC+00:00)');
  assert.equal(timezoneOption('UTC', 'en', now)[1], 'Coordinated Universal Time (UTC+00:00)');
});

test('offsets follow daylight saving time and saved aliases keep their original value', () => {
  assert.equal(timezoneOption('America/New_York', 'zh', new Date('2026-01-15T00:00:00Z'))[1], '纽约 (UTC-05:00)');
  assert.equal(timezoneOption('America/New_York', 'en', new Date('2026-07-15T00:00:00Z'))[1], 'New York (UTC-04:00)');
  assert.deepEqual(timezoneOption('US/Eastern', 'zh', new Date('2026-07-15T00:00:00Z')), ['US/Eastern', '纽约 (UTC-04:00)']);
});
