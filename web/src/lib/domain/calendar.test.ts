import { test } from 'node:test';
import assert from 'node:assert/strict';
import { monthDays, parseDay, presetRange, rangeDays, shiftDay, todayInZone } from './calendar.ts';

test('month grid is Monday-first and handles five/six weeks and leap days', () => {
  const october = monthDays(2026, 9);
  assert.equal(october.length, 35);
  assert.equal(october[0], '2026-09-28');
  assert.equal(october[3], '2026-10-01');
  assert.equal(october[24], '2026-10-22');
  assert.equal(october.at(-1), '2026-11-01');
  assert.equal(monthDays(2026, 7).length, 42);
  assert.ok(monthDays(2024, 1).includes('2024-02-29'));
  assert.ok(!monthDays(2026, 1).includes('2026-02-29'));
});

test('date ranges reject invalid/reversed dates and remain inclusive across DST', () => {
  assert.equal(parseDay('2026-02-29'), null);
  assert.equal(parseDay('2026-13-01'), null);
  assert.equal(parseDay(''), null);
  assert.equal(rangeDays('2026-10-22', '2026-10-19'), 0);
  assert.equal(rangeDays('2026-10-19', '2026-10-22'), 4);
  assert.equal(rangeDays('2026-03-07', '2026-03-09'), 3);
  assert.equal(shiftDay('2026-12-31', 1), '2027-01-01');
});

test('today and quick ranges follow the selected timezone and server boundaries', () => {
  const now = new Date('2026-10-21T17:00:00Z');
  assert.equal(todayInZone('Asia/Shanghai', now), '2026-10-22');
  assert.equal(todayInZone('America/Los_Angeles', now), '2026-10-21');
  assert.deepEqual(presetRange('week', '2026-10-22'), { from: '2026-10-19', to: '2026-10-22' });
  assert.deepEqual(presetRange('7d', '2026-01-03'), { from: '2025-12-28', to: '2026-01-03' });
  assert.deepEqual(presetRange('month', '2026-10-22'), { from: '2026-10-01', to: '2026-10-22' });
  assert.deepEqual(presetRange('all', '2026-10-22', '2025-01-01'), {
    from: '2025-01-01',
    to: '2026-10-22',
  });
});
