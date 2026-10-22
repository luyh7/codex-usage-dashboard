import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const output = 'output/verification/metric-cards';
await mkdir(output, { recursive: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 1024 }, locale: 'zh-CN', timezoneId: 'Asia/Shanghai', reducedMotion: 'reduce' });
await installMintFixture(context);
const page = await context.newPage();
page.setDefaultTimeout(8000);
const report = { checks: [], errors: [] };
page.on('pageerror', error => report.errors.push(String(error)));
const check = label => { report.checks.push(label); console.log(label); };
const tooltip = page.getByRole('tooltip');
async function expectTooltip(parts) {
  await tooltip.waitFor();
  assert.equal(await tooltip.count(), 1, 'Only one chart tooltip should be visible');
  const text = await tooltip.innerText();
  for (const part of parts) assert.ok(text.includes(part), `Missing ${part} in ${text}`);
  const box = await tooltip.boundingBox();
  const viewport = page.viewportSize();
  assert.ok(box.x >= 0 && box.y >= 0 && box.x + box.width <= viewport.width && box.y + box.height <= viewport.height, 'Tooltip stays inside viewport');
}
async function leave() {
  await page.locator('h1').hover();
  assert.equal(await tooltip.count(), 0, 'Tooltip dismisses on pointer exit');
}
try {
  await page.goto('http://127.0.0.1:8765/svelte?period=week&timezone=Asia%2FShanghai');
  await page.locator('.daily-bar').first().waitFor();
  await page.evaluate(() => document.fonts.ready);
  assert.equal((await page.locator('.daily-usage').innerText()).trim(), '');
  assert.equal(await page.locator('.daily-bar').count(), 7);
  assert.ok((await page.locator('.metric-date-range').innerText()).includes('10/19 — 10/25'));
  assert.ok((await page.locator('.cache-counts').innerText()).includes('10.86M'));
  const ring = await page.locator('.cache-ring .ring-value').evaluate(el => ({ stroke: getComputedStyle(el).stroke, cap: getComputedStyle(el).strokeLinecap }));
  assert.deepEqual(ring, { stroke: 'rgb(21, 129, 126)', cap: 'butt' });
  await page.locator('.metrics').screenshot({ path: `${output}/cards.png` });
  check('Selected design includes cost range and axis dates, a flat cache ring and input counts, and daily bars without persistent labels');

  await page.locator('.daily-column').nth(2).hover();
  await expectTooltip(['2026-10-21', '3.85M tokens']);
  await page.screenshot({ path: `${output}/daily-hover.png` });
  await leave();
  await page.locator('.daily-column').last().hover();
  await expectTooltip(['2026-10-25', '0 tokens']);
  await leave();
  await page.locator('.cost-point rect').nth(3).hover();
  await expectTooltip(['2026-10-21', '累计估算费用', '$11.17', '区间费用', '$5.59']);
  await page.screenshot({ path: `${output}/cost-hover.png` });
  await leave();
  // Inspect the plotted origin; the transparent hit region also extends behind the headline.
  const origin = await page.locator('.cost-point rect').first().boundingBox();
  await page.mouse.move(origin.x + origin.width / 2, origin.y + origin.height - 2);
  await expectTooltip(['起点', '$0.00']);
  await leave();
  await page.locator('.session-chart').hover();
  await expectTooltip(['主会话', '8 · 80%', '子代理', '2 · 20%']);
  await leave();
  await page.locator('.cache-chart').hover();
  await expectTooltip(['缓存命中率 · 89%', '10,856,220 tokens', '12,198,000 tokens']);
  await page.screenshot({ path: `${output}/cache-hover.png` });
  await leave();
  check('Hover shows actual daily/zero-day tokens, cumulative and interval costs, session proportions, and cache numerator/denominator');

  await page.locator('.daily-column').first().focus();
  await expectTooltip(['2026-10-19', '1.28M tokens']);
  await page.locator('.cache-chart').hover();
  await expectTooltip(['缓存命中率']);
  await leave();
  await page.locator('.cost-point rect').last().focus();
  await expectTooltip(['累计估算费用', '$18.62']);
  await page.keyboard.press('Escape');
  assert.equal(await tooltip.count(), 0);
  await page.locator('.cache-chart').focus();
  await expectTooltip(['缓存命中率']);
  await page.locator('.calendar-trigger').click();
  assert.equal(await tooltip.count(), 0);
  await page.keyboard.press('Escape');
  check('Keyboard focus exposes the same values; Escape, focus changes, and opening the date modal dismiss tooltips without duplicates');

  for (const width of [1920, 1366, 1101, 800, 390, 320]) {
    await page.setViewportSize({ width, height: width > 800 ? 1024 : 844 });
    await page.locator('.cache-chart').hover();
    await expectTooltip(['缓存命中率', '10,856,220 tokens']);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await leave();
  }
  check('Tooltip bounds and cards fit desktop, laptop, tablet and narrow mobile widths');
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
