import { test } from 'node:test';
import assert from 'node:assert/strict';
import { tooltipPosition } from './tooltip';

const viewport = { width: 800, height: 600 };
const surface = { width: 180, height: 60 };
const anchor = { left: 300, top: 200, width: 100, height: 40 };

test('ordinary hints center above the target and stay fixed as the pointer moves', () => {
  const position = { left: 260, top: 130 };
  assert.deepEqual(tooltipPosition(anchor, surface, viewport), position);
  assert.deepEqual(tooltipPosition(anchor, surface, viewport, { x: 305, y: 220 }), position);
  assert.deepEqual(tooltipPosition(anchor, surface, viewport, { x: 395, y: 230 }), position);
});

test('ordinary hints flip below near the top and keep viewport clearance at both edges', () => {
  assert.deepEqual(tooltipPosition({ ...anchor, top: 20 }, surface, viewport), { left: 260, top: 70 });
  assert.equal(tooltipPosition({ ...anchor, left: 0 }, surface, viewport).left, 8);
  assert.equal(tooltipPosition({ ...anchor, left: 750 }, surface, viewport).left, 612);
});

test('sidebar hints center on the right and fall back to the left', () => {
  assert.deepEqual(tooltipPosition(anchor, surface, viewport, { side: 'right' }), { left: 410, top: 190 });
  assert.deepEqual(tooltipPosition({ ...anchor, left: 650 }, surface, viewport, { side: 'right' }), { left: 460, top: 190 });
});

test('chart hints follow the pointer and flip away from the right and bottom edges', () => {
  assert.deepEqual(tooltipPosition(anchor, surface, viewport, { side: 'pointer', x: 350, y: 220 }), { left: 364, top: 234 });
  assert.deepEqual(tooltipPosition(anchor, surface, viewport, { side: 'pointer', x: 790, y: 590 }), { left: 596, top: 516 });
});

test('keyboard chart hints have an anchored fallback and narrow screens retain 8px clearance', () => {
  assert.deepEqual(tooltipPosition(anchor, surface, viewport, { side: 'pointer' }), { left: 260, top: 130 });
  assert.deepEqual(tooltipPosition(anchor, { width: 304, height: 150 }, { width: 320, height: 400 },
    { side: 'pointer', x: 310, y: 390 }), { left: 8, top: 226 });
});
