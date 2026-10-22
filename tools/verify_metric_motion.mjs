import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture, sessions, summary, usage, stamp } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const output = 'output/verification/metric-motion';
await mkdir(output, { recursive: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 1024 }, locale: 'zh-CN', timezoneId: 'Asia/Shanghai', reducedMotion: 'no-preference' });
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
let scale = 1, generation = 1;
await context.route('**/api/v2/sessions?*', route => {
  const totals = summary(sessions);
  return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({
    page: { query_token: `motion-${generation}`, revision: `motion-${generation}`, items: sessions, summary: totals, total: sessions.length, offset: 0, limit: 100, next_cursor: null, previous_cursor: null },
    facets: totals, generated_at: stamp, codex_sources: [], remotes: [], current_device_short_code: 'this-device',
    period: { key: 'week', start_date: '2026-10-19', end_date: '2026-10-25' },
    metric_trend: { complete: true, points: [1, 2, 3, 4].map((weight, i) => ({ date: `2026-10-${19 + i}`, usage: usage(totals.usage.total_tokens * weight / 10), session_count: sessions.length, estimated_cost_usd: totals.estimated_cost_usd * weight / 10 * scale })) },
  }) });
});
const page = await context.newPage();
page.setDefaultTimeout(8000);
const report = { checks: [], errors: [], port: 8765 };
page.on('pageerror', error => report.errors.push(String(error)));
const check = text => { report.checks.push(text); console.log(text); };
const style = (selector, property) => page.locator(selector).evaluate((node, property) => getComputedStyle(node)[property], property);
const publish = async () => { generation++; await page.evaluate(generation => window.__publishRevision(generation), generation); };
const leave = async () => { await page.locator('h1').hover(); await page.waitForTimeout(260); };
try {
  await page.goto('http://127.0.0.1:8765/svelte?view=sessions&period=week&timezone=Asia%2FShanghai');
  await page.locator('.cost-reveal').waitFor();
  await page.evaluate(() => document.fonts.ready);
  const clips = await page.locator('.cost-reveal').evaluate(node => {
    const animation = node.getAnimations()[0];
    if (!animation) throw new Error('Missing curve reveal animation');
    animation.pause();
    return [0, 325, 650].map(time => {
      animation.currentTime = time;
      return getComputedStyle(node).clipPath;
    });
  });
  assert.deepEqual(clips.map(clip => Number(clip.match(/inset\(0px ([\d.]+)(?:%|px)/)[1])), [100, 50, 0]);
  check('Curve and fill reveal together from left to right over 650ms');
  await page.evaluate(() => { window.__curve = document.querySelector('.cost-reveal'); });
  scale = 2;
  await publish();
  await page.waitForFunction(() => Number(document.querySelector('.cost-point:last-child rect').dataset.cost) > 37);
  assert.equal(await page.evaluate(() => window.__curve === document.querySelector('.cost-reveal')), true);
  assert.equal(await page.locator('.cost-reveal').evaluate(node => node.getAnimations()[0].currentTime), 650);
  await page.waitForTimeout(700);
  await page.evaluate(() => { window.__curve = document.querySelector('.cost-reveal'); });
  const response = page.waitForResponse(response => new URL(response.url()).pathname === '/api/v2/sessions');
  await publish(); await response; await page.waitForTimeout(100);
  assert.equal(await page.evaluate(() => window.__curve === document.querySelector('.cost-reveal')), true);
  check('Changed costs and identical background refreshes retain the curve without replaying its reveal');

  await page.locator('.cost-point rect').nth(3).hover();
  await page.waitForTimeout(200);
  assert.equal(await style('.cost-point rect[data-date="2026-10-21"] + .cost-marker', 'opacity'), '1');
  assert.equal(await page.getByRole('tooltip').count(), 1);
  await page.screenshot({ path: `${output}/cost-hover.png` });
  await leave();
  const bar = '.daily-column:nth-child(3) .daily-bar';
  const baseColor = await style(bar, 'backgroundColor');
  await page.locator('.daily-column').nth(2).hover();
  await page.waitForTimeout(220);
  assert.notEqual(await style(bar, 'transform'), 'none');
  assert.notEqual(await style(bar, 'backgroundColor'), baseColor);
  await page.screenshot({ path: `${output}/tokens-hover.png` });
  await leave();
  assert.equal(await style(bar, 'transform'), 'none');
  for (const [anchor, ring, segment, width] of [
    ['.session-chart', '.session-ring', '.session-agent-segment', '7.5px'],
    ['.cache-chart', '.cache-ring', '.ring-value', '7px'],
  ]) {
    const baseline = await style(ring, 'transform');
    await page.locator(anchor).hover();
    await page.waitForTimeout(260);
    assert.notEqual(await style(ring, 'transform'), baseline);
    assert.equal(await style(segment, 'strokeWidth'), width);
    assert.equal(await page.getByRole('tooltip').count(), 1);
    await page.screenshot({ path: `${output}/${anchor.slice(1)}-hover.png` });
    await leave();
    assert.equal(await style(ring, 'transform'), baseline);
    await page.keyboard.press('Tab');
    await page.locator(anchor).focus();
    await page.waitForTimeout(260);
    assert.notEqual(await style(ring, 'transform'), baseline);
    await page.locator(anchor).evaluate(node => node.blur());
    await leave();
  }
  await page.locator('.cost-point rect').nth(3).focus();
  await page.waitForTimeout(200);
  assert.equal(await style('.cost-point rect[data-date="2026-10-21"] + .cost-marker', 'opacity'), '1');
  await page.locator('.daily-column').nth(2).focus();
  await page.waitForTimeout(220);
  assert.notEqual(await style(bar, 'transform'), 'none');
  await page.locator('.daily-column').nth(2).evaluate(node => node.blur());
  await leave();
  check('All four charts animate on hover and keyboard focus, retain tooltips, and return to rest on exit');

  await page.emulateMedia({ reducedMotion: 'reduce' });
  assert.equal(await page.locator('.cost-reveal').evaluate(node => node.getAnimations().length), 0);
  await page.locator('.daily-column').nth(2).hover();
  assert.equal(await style(bar, 'transform'), 'none');
  assert.equal(await style(bar, 'transitionDuration'), '0s');
  await leave();
  for (const [anchor, ring] of [['.session-chart', '.session-ring'], ['.cache-chart', '.cache-ring']]) {
    const baseline = await style(ring, 'transform');
    await page.locator(anchor).hover();
    assert.equal(await style(ring, 'transform'), baseline);
    assert.equal(await style(ring, 'transitionDuration'), '0s');
    await leave();
  }
  check('Reduced motion disables reveals and scaling while preserving static highlights');
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
