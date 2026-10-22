import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture, session, summary, usage } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const origin = 'http://127.0.0.1:8765';
const output = 'output/verification/hourly-metrics';
await mkdir(output, { recursive: true });
const report = { checks: [], errors: [] };
const context = await browser.newContext({ viewport: { width: 1536, height: 1024 }, locale: 'zh-CN', timezoneId: 'Asia/Shanghai', reducedMotion: 'reduce' });
const page = await context.newPage();
page.setDefaultTimeout(15000);
page.on('pageerror', error => report.errors.push(String(error)));
try {
  const live = await (await context.request.get(`${origin}/api/v2/projects?period=today&timezone=Asia%2FShanghai`)).json();
  assert.equal(live.metric_trend.granularity, 'hour');
  if (live.metric_trend.complete) {
    assert.equal(live.metric_trend.points.reduce((total, point) => total + point.usage.total_tokens, 0), live.page.summary.usage.total_tokens);
  }
  await page.goto(`${origin}/svelte?period=today&timezone=Asia%2FShanghai`);
  await page.locator('.daily-bar').first().waitFor();
  assert.equal(await page.locator('.daily-bar').count(), live.metric_trend.points.length);
  assert.equal(await page.locator('.daily-usage').getAttribute('aria-label'), '每小时用量');
  await page.evaluate(() => document.fonts.ready);
  await page.locator('.metrics').screenshot({ path: `${output}/live-today.png` });
  report.checks.push('Live hourly usage matches the complete selected-day total');

  await installMintFixture(context);
  await context.route('**/api/v2/projects?*', async route => {
    const params = new URL(route.request().url()).searchParams;
    if (params.get('start') !== '2026-10-21' || params.get('end') !== '2026-10-21') return route.fallback();
    const row = session('hourly', 'Hourly fixture', 400, .4);
    const s = summary([row]);
    const points = Array.from({ length: 24 }, (_, hour) => {
      const tokens = hour === 0 ? 100 : hour === 8 ? 300 : 0;
      return {
        date: `2026-10-21T${String(hour).padStart(2, '0')}:00:00+08:00`,
        end: hour === 23 ? '2026-10-22T00:00:00+08:00' : `2026-10-21T${String(hour + 1).padStart(2, '0')}:00:00+08:00`,
        usage: usage(tokens), session_count: tokens ? 1 : 0, estimated_cost_usd: tokens / 1000,
      };
    });
    return route.fulfill({ json: {
      page: { query_token: 'hourly', revision: 'hourly', items: [], summary: s, total: 0, offset: 0, limit: 100, next_cursor: null, previous_cursor: null },
      metric_trend: { complete: true, granularity: 'hour', points }, facets: s,
      period: { key: 'custom', start_date: '2026-10-21', end_date: '2026-10-21', start_at: '2026-10-20T16:00:00Z', end_at: '2026-10-21T15:59:59.999999Z' },
      generated_at: '2026-10-22T01:00:00Z', codex_sources: [], remotes: [], current_device_short_code: 'hourly',
    } });
  });
  await page.goto(`${origin}/svelte?period=custom&start=2026-10-21&end=2026-10-21&timezone=Asia%2FShanghai`);
  await page.locator('.cost-point rect').last().waitFor();
  assert.equal(await page.locator('.daily-bar').count(), 24);
  assert.equal(await page.locator('.cost-point rect').count(), 25);
  assert.equal(await page.locator('.cost-spark text, .spark-axis').count(), 0);
  assert.equal(await page.locator('.cost-point rect').last().getAttribute('data-cost'), '0.4');
  await page.locator('.metrics').screenshot({ path: `${output}/historical-day.png` });
  await page.locator('.daily-column').nth(8).hover();
  await page.getByRole('tooltip').waitFor();
  assert.match(await page.getByRole('tooltip').innerText(), /08:00 – 09:00/);
  assert.match(await page.getByRole('tooltip').innerText(), /300 tokens/);
  await page.locator('.daily-column').nth(7).focus();
  assert.match(await page.getByRole('tooltip').innerText(), /07:00 – 08:00/);
  assert.match(await page.getByRole('tooltip').innerText(), /0 tokens/);
  await page.locator('.cost-point rect').nth(9).focus();
  assert.match(await page.getByRole('tooltip').innerText(), /08:00 – 09:00/);
  assert.match(await page.getByRole('tooltip').innerText(), /\$0.40/);
  assert.match(await page.getByRole('tooltip').innerText(), /\$0.30/);
  report.checks.push('A historical day shows 24 hourly bars, cumulative cost and accurate zero/nonzero hour tooltips for pointer and keyboard');
  await page.goto(`${origin}/svelte?period=week&timezone=Asia%2FShanghai`);
  await page.locator('.daily-bar').first().waitFor();
  assert.equal(await page.locator('.daily-bar').count(), 7);
  assert.equal(await page.locator('.cost-spark text, .spark-axis').count(), 0);
  report.checks.push('Multiple days retain daily bars; cost sparklines have no persistent numbers');
  for (const [width, height] of [[1920, 1080], [1536, 1024], [1366, 768], [1101, 768], [1024, 768], [800, 844], [1536, 600]]) {
    await page.setViewportSize({ width, height });
    await page.waitForFunction(() => {
      const svg = document.querySelector('.cost-spark svg');
      const box = svg.getBoundingClientRect();
      const view = svg.viewBox.baseVal;
      return Math.abs(view.width - box.width) < 1.1 && Math.abs(view.height - box.height) < 1.1;
    });
    assert.equal(await page.locator('.cost-spark circle').count(), 0, 'Cost curve has no point markers');
    const geometry = await page.evaluate(() => {
      const svg = document.querySelector('.cost-spark svg');
      const paths = svg.querySelectorAll('path');
      const box = svg.getBoundingClientRect();
      const bars = [...document.querySelectorAll('.daily-bar')].map(bar => bar.getBoundingClientRect());
      return {
        curve: paths[1].getAttribute('d'),
        fill: paths[0].getAttribute('fill'),
        stops: [...svg.querySelectorAll('stop')].map(stop => Number(stop.getAttribute('stop-opacity'))),
        // The stroke extends 1.2px below its zero origin to the SVG bottom edge.
        baseline: box.bottom,
        barBottoms: bars.map(bar => bar.bottom),
      };
    });
    assert.ok(geometry.curve.includes('C') && !geometry.curve.includes('L'), 'Smooth cubic cost curve');
    assert.match(geometry.fill, /^url\(#/);
    assert.ok(geometry.stops[0] > geometry.stops.at(-1), 'Area fades to transparent at the baseline');
    for (const bottom of geometry.barBottoms) assert.ok(Math.abs(bottom - geometry.baseline) < 1, `Aligned chart bottoms at ${width}×${height}`);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    await page.locator('.cost-card').screenshot({ path: `${output}/cost-${width}x${height}.png` });
  }
  report.checks.push('Smooth cost curve has no markers, uses a fading gradient, and aligns with daily bars at seven desktop/tablet/short-screen sizes without overflow');
  assert.deepEqual(report.errors, []);
  report.passed = true;
  console.log(JSON.stringify(report));
} catch (error) {
  report.failure = String(error);
  await page.screenshot({ path: `${output}/failure.png` });
  throw error;
} finally {
  await writeFile(`${output}/checks.json`, JSON.stringify(report, null, 2));
  await browser.close();
}
