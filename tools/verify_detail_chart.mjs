import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const origin = 'http://127.0.0.1:8765';
const output = 'output/verification/detail-chart';
await mkdir(output, { recursive: true });
const context = await browser.newContext({ viewport: { width: 1920, height: 1080 }, locale: 'zh-CN', timezoneId: 'Asia/Shanghai', reducedMotion: 'reduce' });
await installMintFixture(context);
await context.addInitScript(() => {
  // Observe real canvas frames without changing geometry or rendering.
  for (const method of ['clearRect', 'arc', 'fillRect', 'fillText']) {
    const original = CanvasRenderingContext2D.prototype[method];
    CanvasRenderingContext2D.prototype[method] = function (...args) {
      if (method === 'clearRect') this.canvas.__draw = { arcs: 0, bars: [], labels: [] };
      const draw = this.canvas.__draw;
      if (draw) {
        if (method === 'arc') draw.arcs++;
        if (method === 'fillRect') draw.bars.push(args);
        if (method === 'fillText') draw.labels.push([...args, this.textAlign]);
      }
      return original.apply(this, args);
    };
  }
  const cubic = Path2D.prototype.bezierCurveTo;
  Path2D.prototype.bezierCurveTo = function (...args) {
    window.__curveCount = (window.__curveCount || 0) + 1;
    return cubic.apply(this, args);
  };
});
let mode = 'midnight';
const cumulative = [28000, 64000, 90000, 151000, 166000, 203000, 245000, 289000, 301000, 346000, 418000, 484310];
const samples = {
  midnight: ['2026-10-21T12:17:00Z', '2026-10-21T16:42:00Z'],
  today: ['2026-10-22T01:00:00Z', '2026-10-22T03:00:00Z'],
  history: ['2026-10-20T01:00:00Z', '2026-10-20T03:00:00Z'],
  days: ['2026-10-19T01:00:00Z', '2026-10-22T01:00:00Z'],
  year: ['2025-12-31T15:00:00Z', '2025-12-31T17:00:00Z'],
};
await context.route('**/api/v2/chart?*', route => {
  const [start, end] = samples[mode] || samples.midnight;
  let timeline = cumulative.map((total_tokens, i) => ({
    timestamp: new Date(Date.parse(start) + (Date.parse(end) - Date.parse(start)) * i / (cumulative.length - 1)).toISOString(),
    total_token_usage: { total_tokens },
  }));
  if (mode === 'duplicates') timeline[1].timestamp = timeline[0].timestamp;
  if (mode === 'undated') timeline = timeline.map(row => ({ ...row, timestamp: null }));
  if (mode === 'single') timeline = timeline.slice(0, 1);
  if (mode === 'empty') timeline = [];
  return route.fulfill({ json: { id: 'main', revision: 'clear-mint-design-v2', timeline } });
});
const page = await context.newPage();
page.setDefaultTimeout(12000);
await page.clock.setFixedTime(new Date('2026-10-22T04:00:00Z'));
const report = { checks: [], errors: [], port: 8765 };
page.on('pageerror', error => report.errors.push(String(error)));
const canvas = page.locator('.usage-chart canvas');
const hint = page.locator('.chart-tooltip');
const frame = () => canvas.evaluate(el => el.__draw);
const labels = async () => (await frame()).labels.map(row => row[0]);
const navigate = async (next = mode, zone = 'Asia/Shanghai') => {
  mode = next;
  await page.goto(`${origin}/?period=week&session=main&timezone=${encodeURIComponent(zone)}`);
  await page.locator('.detail-panel[aria-busy="false"]').waitFor();
  await page.evaluate(() => document.fonts.ready);
  if (mode !== 'empty') await page.waitForFunction(() => document.querySelector('.usage-chart canvas')?.__draw?.labels.length > 0);
};
const check = text => { report.checks.push(text); console.log(text); };
try {
  report.health = await (await context.request.get(`${origin}/api/health`)).json();
  assert.equal(report.health.app, 'codex-usage-dashboard');
  await navigate();
  assert.equal((await frame()).arcs, 0);
  assert.equal(await hint.count(), 0);
  assert.ok((await frame()).bars.length > 0);
  assert.ok(await page.evaluate(() => window.__curveCount > 0));
  assert.ok((await labels()).includes('10/21'));
  assert.ok((await labels()).includes('10/22'));
  await page.locator('.detail-panel').screenshot({ path: `${output}/midnight-rest.png` });
  check('Cross-midnight chart renders date labels, cubic segments and interval bars, with no resting nodes or tooltip');

  let box = await canvas.boundingBox();
  await page.mouse.move(box.x + box.width - 10, box.y + 40);
  await hint.waitFor();
  assert.equal((await frame()).arcs, 2);
  assert.match(await hint.innerText(), /10\/22 00:42/);
  assert.match(await hint.innerText(), /484\.31K/);
  assert.match(await hint.innerText(), /分段用量/);
  assert.match(await hint.innerText(), /66\.31K/);
  await page.locator('.detail-panel').screenshot({ path: `${output}/midnight-hover.png` });
  await page.mouse.move(30, 20);
  await hint.waitFor({ state: 'detached' });
  assert.equal((await frame()).arcs, 0);
  await canvas.focus();
  await canvas.press('Home');
  assert.match(await hint.innerText(), /10\/21 20:17/);
  await canvas.press('End');
  assert.match(await hint.innerText(), /10\/22 00:42/);
  await canvas.press('Escape');
  assert.equal(await hint.count(), 0);
  assert.equal((await frame()).arcs, 0);
  assert.equal(await page.locator('.detail-panel').count(), 1);
  check('Pointer and keyboard reveal only the inspected node and exact date/usage; leaving or Escape clears it');

  for (const [next, expected, absent] of [
    ['today', ['09:00', '11:00'], ['10/22']],
    ['history', ['10/20', '09:00'], []],
    ['days', ['10/19', '10/22'], ['09:00']],
    ['year', ['2025/12/31', '2026/01/01'], []],
    ['undated', ['#1'], []],
  ]) {
    await navigate(next);
    const text = await labels();
    for (const value of expected) assert.ok(text.includes(value), `${next}: ${value}`);
    for (const value of absent) assert.ok(!text.includes(value), `${next} should omit ${value}`);
    assert.equal((await frame()).arcs, 0);
  }
  await navigate('midnight', 'UTC');
  assert.ok((await labels()).includes('10/21'));
  assert.ok(!(await labels()).includes('10/22'));
  check('Today, historical days, multiple days, cross-year and undated imports follow display-timezone date rules');

  for (const next of ['duplicates', 'single']) {
    await navigate(next);
    assert.ok((await frame()).bars.length > 0);
    assert.equal((await frame()).arcs, 0);
    await canvas.focus();
    await canvas.press('End');
    assert.ok(await hint.isVisible());
  }
  await navigate('empty');
  assert.equal(await canvas.count(), 0);
  assert.equal(await hint.count(), 0);
  check('Repeated timestamps, a single event and empty history render without stale nodes or errors');

  await navigate('year');
  for (const [width, height] of [[1536, 1024], [1101, 768], [800, 844], [390, 844], [320, 740]]) {
    await page.setViewportSize({ width, height });
    await page.mouse.move(0, 0);
    await canvas.scrollIntoViewIfNeeded();
    await page.waitForFunction(() => {
      const canvas = document.querySelector('.usage-chart canvas');
      return Math.abs(canvas.width / devicePixelRatio - canvas.clientWidth) < 1;
    });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    const overlaps = await canvas.evaluate(el => {
      const ctx = el.getContext('2d');
      const dates = el.__draw.labels.filter(([text]) => /^\d{4}\//.test(text)).map(([text, x, y, align]) => {
        const width = ctx.measureText(text).width;
        const left = x - (align === 'center' ? width / 2 : align === 'right' ? width : 0);
        return { left, right: left + width, y };
      });
      return dates.some((date, i) => i && Math.abs(date.y - dates[i - 1].y) < 1 && date.left < dates[i - 1].right + 4);
    });
    assert.equal(overlaps, false, `Year labels do not overlap at ${width}px`);
    box = await canvas.boundingBox();
    await page.mouse.move(box.x + box.width - 10, box.y + 35);
    await hint.waitFor();
    const hintBox = await hint.boundingBox();
    assert.ok(hintBox.x >= 0 && hintBox.x + hintBox.width <= width + 1, `Tooltip fits ${width}px`);
    await page.locator('.usage-chart').screenshot({ path: `${output}/chart-${width}.png` });
  }
  check('Chart, dates and hover hints fit desktop, tablet and 320px mobile layouts');
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
