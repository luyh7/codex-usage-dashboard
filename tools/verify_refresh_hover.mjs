import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture, sessions, summary, usage, stamp } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const output = 'output/verification/refresh-hover';
await mkdir(output, { recursive: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 1024 }, locale: 'zh-CN', timezoneId: 'Asia/Shanghai' });
await installMintFixture(context);
await context.addInitScript(() => {
  const streams = new Set();
  window.EventSource = class extends EventTarget {
    constructor() { super(); streams.add(this); }
    close() { streams.delete(this); }
  };
  window.__publishRevision = revision => {
    for (const stream of streams) stream.dispatchEvent(new MessageEvent('revision', { data: JSON.stringify({ phase: 'ready', revision }) }));
  };
});
let generation = 1, scale = 1, firstHour = 0, known = true, empty = false, changedEnd = false;
const hour = n => n === 24 ? '2026-10-23T00:00:00+08:00' : `2026-10-22T${String(n).padStart(2, '0')}:00:00+08:00`;
await context.route('**/api/v2/sessions?*', route => {
  const totals = summary(sessions);
  totals.usage = usage(totals.usage.total_tokens * scale);
  totals.estimated_cost_usd *= scale;
  totals.main_count += scale;
  totals.session_count += scale;
  return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({
    page: { query_token: `hover-${generation}`, revision: `hover-${generation}`, items: sessions, summary: totals, total: sessions.length, offset: 0, limit: 100, next_cursor: null, previous_cursor: null },
    facets: totals, generated_at: stamp, codex_sources: [], remotes: [], current_device_short_code: 'this-device',
    period: { key: 'today', start_date: '2026-10-22', end_date: '2026-10-22' },
    metric_trend: { complete: true, granularity: 'hour', points: empty ? [] : Array.from({ length: 24 - firstHour }, (_, n) => {
      const i = n + firstHour;
      return { date: hour(i), end: hour(i === 11 && changedEnd ? 13 : i + 1), usage: usage((i + 1) * 100 * scale), session_count: i, estimated_cost_usd: known ? (i === 23 ? scale * scale : (i + 1) / 10) : null };
    }) },
  }) });
});
const page = await context.newPage();
page.setDefaultTimeout(8000);
const report = { checks: [], errors: [], port: 8765 };
page.on('pageerror', error => report.errors.push(String(error)));
const check = label => { report.checks.push(label); console.log(label); };
const tooltip = page.getByRole('tooltip');
const bar = n => page.locator(`.daily-column:has(.daily-bar[data-date="${hour(n)}"])`);
const point = n => page.locator(`.cost-point rect[data-date="${hour(n)}"]`);
const publish = async (poll = false) => {
  generation++;
  const response = page.waitForResponse(r => new URL(r.url()).pathname === '/api/v2/sessions');
  if (poll) await page.clock.fastForward(15000);
  else await page.evaluate(n => window.__publishRevision(n), generation);
  await response;
  // Include delegated tooltip's delay and a full render, with the pointer stationary.
  await page.waitForTimeout(250);
};
const remember = async anchor => {
  await anchor.evaluate(node => {
    window.__anchor = node;
    window.__tooltip = document.querySelector('[role="tooltip"]');
    window.__curve = document.querySelector('.cost-reveal');
    window.__curvePath = document.querySelector('.cost-reveal path')?.getAttribute('d');
    window.__description = node.getAttribute('aria-describedby');
  });
};
const retained = async (anchor, structured = true) => {
  assert.equal(await tooltip.count(), 1);
  assert.equal(await anchor.evaluate(node => node === window.__anchor), true, 'Keep the same time interval/focus target');
  assert.equal(await tooltip.evaluate(node => node === window.__tooltip), true, 'Update the existing tooltip surface');
  assert.equal(await anchor.getAttribute('aria-describedby'), await anchor.evaluate(() => window.__description));
  if (structured) {
    assert.equal(await tooltip.getAttribute('class'), 'metric-tooltip');
    assert.ok(await tooltip.locator('.metric-tooltip-row').count());
    assert.equal(await page.locator('.ui-tooltip').count(), 0);
  }
};
const leave = async () => { await page.mouse.move(900, 60); await page.locator('h1').click(); };
try {
  await page.clock.install();
  await page.goto('http://127.0.0.1:8765/svelte?view=sessions&period=today&timezone=Asia%2FShanghai');
  await bar(11).waitFor();
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(750);
  await bar(11).hover();
  await page.waitForTimeout(250);
  assert.equal(await tooltip.getAttribute('class'), 'metric-tooltip', 'No generic aria-label fallback, even after its hover delay');
  await remember(bar(11));
  const before = await tooltip.boundingBox();
  scale = 2;
  await publish();
  await retained(bar(11));
  assert.match(await tooltip.innerText(), /2\.4K tokens/);
  assert.match(await tooltip.innerText(), /11:00 – 12:00/);
  const after = await tooltip.boundingBox();
  assert.ok(Math.abs(before.x - after.x) < 1 && Math.abs(before.y - after.y) < 1);
  assert.equal(await bar(11).evaluate(node => node.matches(':hover')), true);
  assert.equal(await page.evaluate(() => window.__curve === document.querySelector('.cost-reveal')), true);
  assert.equal(await page.evaluate(() => window.__curvePath !== document.querySelector('.cost-reveal path').getAttribute('d')), true, 'A changed curve is drawn in place');
  assert.equal(await page.locator('.cost-reveal').evaluate(node => node.getAnimations().some(a => a.playState === 'running')), false);
  await page.screenshot({ path: `${output}/hourly-hover-after-refresh.png` });
  await publish();
  await retained(bar(11));
  check('Stationary hourly hover keeps its structured surface, position, highlight and interval while values update; equal refreshes stay still');

  await leave();
  const cost = point(12);
  await cost.hover();
  const box = await cost.boundingBox();
  await page.mouse.move(box.x + box.width - 2, box.y + box.height / 2);
  await remember(cost);
  const costBefore = await tooltip.boundingBox();
  scale = 3;
  await publish();
  await retained(cost);
  const costAfter = await tooltip.boundingBox();
  assert.ok(Math.abs(costBefore.x + costBefore.width / 2 - costAfter.x - costAfter.width / 2) < 1, 'Content updates keep the latest pointer X');
  assert.equal(await page.locator('.cost-point:has(rect[data-date="' + hour(12) + '"]) .cost-marker').evaluate(node => getComputedStyle(node).opacity), '1');
  check('Cost updates keep the curve node, hovered marker and latest pointer position without replaying entry animation');

  await leave();
  await bar(11).focus();
  await remember(bar(11));
  firstHour = 1;
  scale = 4;
  await publish();
  await retained(bar(11));
  assert.equal(await bar(11).evaluate(node => document.activeElement === node), true);
  assert.match(await tooltip.innerText(), /4\.8K tokens/);
  await leave();
  await point(12).focus();
  await remember(point(12));
  firstHour = 2;
  await publish();
  await retained(point(12));
  assert.equal(await point(12).evaluate(node => document.activeElement === node), true);
  assert.match(await tooltip.innerText(), /11:00 – 12:00/);
  check('Retained time buckets keep keyboard focus and tooltip identity when earlier buckets disappear');

  await leave();
  await bar(2).hover();
  await tooltip.waitFor();
  firstHour = 3;
  await publish();
  assert.equal(await bar(2).count(), 0);
  assert.equal(await page.locator('.ui-tooltip').count(), 0);
  // The browser may enter the replacement bucket under the pointer; any hint must describe it.
  const hovered = page.locator('.daily-column:hover .daily-bar');
  if (await tooltip.count()) {
    assert.equal(await tooltip.getAttribute('class'), 'metric-tooltip');
    const date = await hovered.getAttribute('data-date');
    assert.match(await tooltip.innerText(), new RegExp(date.slice(11, 16)));
  }
  await leave();
  await bar(11).hover();
  firstHour = 10;
  await publish();
  if (await tooltip.count()) {
    const date = await page.locator('.daily-column:hover .daily-bar').getAttribute('data-date');
    assert.match(await tooltip.innerText(), new RegExp(date.slice(11, 16)), 'A changed hit area follows the actual pointer, not the previously hovered interval');
  }
  await leave();
  await point(12).focus();
  known = false;
  await publish();
  assert.equal(await page.locator('.cost-point').count(), 0);
  assert.equal(await tooltip.count(), 0);
  assert.equal(await page.evaluate(() => window.__anchor?.hasAttribute('aria-describedby')), false);
  check('Removed intervals and unavailable cost data clean up tooltips and ARIA without showing stale bucket values');

  for (const selector of ['.session-chart', '.cache-chart']) {
    await leave();
    const anchor = page.locator(selector);
    await anchor.hover();
    await page.waitForTimeout(250);
    await remember(anchor);
    const old = await tooltip.innerText();
    scale++;
    await publish();
    await retained(anchor);
    assert.notEqual(await tooltip.innerText(), old);
  }
  check('Session and cache ring hover surfaces update in place with live values');

  await leave();
  const total = page.locator('.token-card .metric-value strong');
  await total.hover();
  await tooltip.waitFor();
  await remember(total);
  scale++;
  await publish();
  await retained(total, false);
  assert.equal(await tooltip.innerText(), new Intl.NumberFormat('zh-CN').format(summary(sessions).usage.total_tokens * scale));
  check('Ordinary abbreviated-value hints also update in place');

  await leave();
  await bar(11).hover();
  await page.waitForTimeout(250);
  await remember(bar(11));
  scale++;
  await publish(true);
  await retained(bar(11));
  assert.match(await tooltip.innerText(), /9\.6K tokens/);
  await page.keyboard.press('Escape');
  assert.equal(await tooltip.count(), 0);
  scale++;
  await publish();
  assert.equal(await tooltip.count(), 0, 'Refreshing cannot reopen an escaped tooltip');
  await page.mouse.move(900, 60);
  await bar(11).hover();
  await tooltip.waitFor();
  await leave();
  await bar(11).focus();
  await remember(bar(11));
  changedEnd = true;
  await publish();
  assert.equal(await tooltip.count(), 0);
  assert.equal(await page.evaluate(() => window.__anchor.isConnected), false, 'A changed interval end must not repurpose the focused bucket');
  assert.equal(await bar(11).locator('.daily-bar').getAttribute('data-end'), hour(13));
  check('Hit area changes follow the physical pointer, and changed interval boundaries dismiss the old inspection');
  await bar(11).hover();
  await tooltip.waitFor();
  empty = true;
  await publish();
  assert.equal(await tooltip.count(), 0);
  assert.equal(await page.locator('.daily-column').count(), 0);
  check('The actual 15-second polling path preserves hover; Escape and empty data remain dismissed across later updates');
  assert.deepEqual(report.errors, []);
  report.passed = true;
} catch (error) {
  report.failure = String(error);
  await page.screenshot({ path: `${output}/failure.png` });
  throw error;
} finally {
  await writeFile(`${output}/checks.json`, JSON.stringify(report, null, 2));
  await browser.close();
}
