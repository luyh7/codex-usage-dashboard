import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ringArc } from './ring.ts';

test('soft ring paths keep the radius and close full circles without a degenerate arc', () => {
  assert.equal(ringArc(0), '');
  assert.equal(ringArc(NaN), '');
  assert.equal(ringArc(-1), '');
  assert.equal(ringArc(25), 'M 39 22 A 17 17 0 0 1 22 39');
  assert.equal(ringArc(75), 'M 39 22 A 17 17 0 1 1 22 5');
  assert.equal(ringArc(25, 75), 'M 22 5 A 17 17 0 0 1 39 22');
  assert.equal(ringArc(100), 'M 39 22 A 17 17 0 1 1 5 22 A 17 17 0 1 1 39 22');
  assert.equal(ringArc(200), ringArc(100));
});
