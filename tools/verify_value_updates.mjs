import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture, sessions, summary, usage, stamp } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const output = 'output/verification/value-updates';
await mkdir(output, { recursive: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 1024 }, locale: 'zh-CN', timezoneId: 'Asia/Shanghai', reducedMotion: 'no-preference' });
await installMintFixture(context);
await context.addInitScript(() => {
  const streams = new Set();
  window.EventSource = class extends EventTarget {
    constructor() { super(); streams.add(this); queueMicrotask(() => this.onopen?.(new Event('open'))); }
    close() { streams.delete(this); }
  };
  window.__publishIndexStatus = value => {
    for (const stream of streams) stream.dispatchEvent(new MessageEvent('revision', { data: JSON.stringify(value) }));
  };
});
const rows = structuredClone(sessions);
const main = rows.find(row => row.uid === 'main');
let generation = 1, dailyTokens = 2140000;
await context.route('**/api/**', async route => {
  const url = new URL(route.request().url()), p = url.searchParams, path = url.pathname;
  const revision = `animated-${generation}`;
  const totals = summary(rows);
  const json = value => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(value) });
  const page = items => ({ revision, query_token: `query-${generation}`, items, summary: totals, total: items.length, offset: 0, limit: 100, next_cursor: null, previous_cursor: null });
  const listing = items => ({ page: page(items), facets: totals, period: { key: p.get('period'), start_date: '2026-10-19', end_date: '2026-10-25' }, generated_at: stamp, codex_sources: [], remotes: [], current_device_short_code: 'this-device' });
  if (path === '/api/v2/projects') return json(listing(['web-app', 'data-tools', 'docs-site'].map(key => ({ key, label: key, ...summary(rows.filter(row => row.project === key)), environments: ['Local'] }))));
  if (path === '/api/v2/sessions') return json(listing(rows));
  if (path === '/api/v2/project-tasks') return json(page(rows.filter(row => !row.is_subagent && row.project === p.get('project')).map(root => {
    const children = rows.filter(row => row.uid === root.uid || row.parent_thread_id === root.uid);
    return { identity: root.uid, sessions: children, summary: summary(children) };
  })));
  const row = rows.find(row => row.uid === p.get('id')) || main;
  const timeline = [0, 1].map(weight => ({ timestamp: stamp, model: row.model, total_token_usage: usage(row.total_token_usage.total_tokens * weight) }));
  if (path === '/api/v2/session') return json({ ...row, revision });
  if (path === '/api/v2/chart') return json({ id: row.uid, revision, timeline });
  if (path === '/api/v2/events') return json({ id: row.uid, revision, events: timeline, offset: 0, total: timeline.length, limit: 100, next_cursor: null, previous_cursor: null });
  if (path === '/api/daily-usage') return json({ generated_at: stamp, daily_usage_complete: true, daily_usage: [{ date: '2026-10-19', usage: usage(dailyTokens) }, { date: '2026-10-20', usage: usage(3460000) }] });
  return route.fallback();
});
const page = await context.newPage();
page.setDefaultTimeout(8000);
await page.clock.setFixedTime(new Date('2026-10-22T04:00:00Z'));
const report = { checks: [], errors: [], port: 8765 };
page.on('pageerror', error => report.errors.push(String(error)));
const check = label => { report.checks.push(label); console.log(label); };
const valueText = selector => `${selector} .animated-value > [aria-hidden="true"]`;
const targets = {
  cost: valueText('.cost-card strong'), totalTokens: valueText('.metrics article:nth-child(2) strong'),
  sessions: valueText('.metrics article:nth-child(3) strong'), cache: valueText('.cache-value strong'),
  projectTokens: valueText('.project-block .project-heading > .numeric:not(.project-cost)'),
  groupTokens: valueText('.group-row > .numeric:not(.cost-cell)'),
  ownTokens: valueText('.agent-row.own > .numeric:not(.cost-cell)'), ownCost: valueText('.agent-row.own > .cost-cell'),
  childTokens: valueText('.agent-row[data-uid="agent-a"] > .numeric:not(.cost-cell)'),
  detailTokens: valueText('.detail-usage'), detailCost: valueText('.detail-cost strong'),
  day: valueText('[data-date="2026-10-19"] small'), unchangedDay: valueText('[data-date="2026-10-20"] small'),
  month: valueText('.calendar-totals b:first-of-type'), year: valueText('.calendar-totals b:last-of-type'),
};
const monitor = selectors => page.evaluate(selectors => {
  window.__numericObserver?.disconnect();
  window.__numericTrace = [];
  const record = () => window.__numericTrace.push({ time: performance.now(), ...Object.fromEntries(Object.entries(selectors).map(([key, selector]) => [key, document.querySelector(selector)?.textContent])) });
  window.__numericObserver = new MutationObserver(record);
  window.__numericObserver.observe(document.querySelector('main'), { childList: true, characterData: true, subtree: true });
  record();
}, selectors);
const trace = () => page.evaluate(() => window.__numericTrace);
const distinct = (frames, key) => [...new Set(frames.map(frame => frame[key]))];
const waitValues = values => page.waitForFunction(({ values, targets }) => Object.entries(values).every(([key, value]) => document.querySelector(targets[key])?.textContent === value), { values, targets });
const publish = async () => {
  generation++;
  await page.evaluate(revision => window.__publishIndexStatus({ phase: 'ready', revision }), generation);
};
try {
  await page.goto('http://127.0.0.1:8765/?period=week&session=main&timezone=Asia%2FShanghai');
  await page.locator('.agent-row.own').waitFor();
  await page.locator('.cost-bar').waitFor();
  await page.locator('.calendar-trigger').click();
  await waitValues({ day: '2.14M', ownTokens: '4.20M', detailTokens: '4.20M', totalTokens: '12.84M' });
  await page.getByLabel('结束日期', { exact: true }).focus();
  await monitor(targets);
  main.total_token_usage = usage(4800000); dailyTokens = 3140000;
  await publish();
  await waitValues({ day: '3.14M', month: '6.60M', year: '6.60M', ownTokens: '4.80M', groupTokens: '5.80M', projectTokens: '9.00M', detailTokens: '4.80M', totalTokens: '13.44M' });
  const changed = await trace();
  for (const key of ['day', 'month', 'year', 'ownTokens', 'groupTokens', 'projectTokens', 'detailTokens', 'totalTokens']) {
    assert.ok(distinct(changed, key).length > 3, `${key} must show intermediate numbers`);
  }
  for (const key of ['cost', 'ownCost', 'detailCost', 'sessions', 'cache', 'childTokens', 'unchangedDay']) {
    assert.equal(distinct(changed, key).length, 1, `${key} must stay still when its value has not changed`);
  }
  assert.equal(await page.getByLabel('结束日期', { exact: true }).evaluate(el => el === document.activeElement), true);
  report.changedFrames = changed;
  check('Changed metric, project, task, detail and calendar totals count toward their new values; unrelated costs, counts, cache and dates stay still');

  await monitor(targets);
  main.total_token_usage = usage(4800001); main.estimated_cost_usd += .000001; dailyTokens++;
  const dailyResponse = page.waitForResponse(response => new URL(response.url()).pathname === '/api/daily-usage');
  await publish(); await dailyResponse; await page.waitForTimeout(550);
  const rounded = await trace();
  for (const key of Object.keys(targets)) assert.equal(distinct(rounded, key).length, 1, `${key} must not animate when the formatted value is unchanged`);
  check('Updates below the displayed precision and identical values do not replay any number animation');

  main.total_token_usage = usage(6200000);
  await publish();
  const ownValue = page.locator('.agent-row.own > .numeric:not(.cost-cell) .animated-value');
  await page.waitForFunction(() => document.querySelector('.agent-row.own > .numeric:not(.cost-cell) .animated-value')?.getAttribute('aria-label') === '6.20M');
  await page.waitForTimeout(70);
  const beforeInterrupt = parseFloat(await ownValue.innerText());
  main.total_token_usage = usage(7100000);
  await publish();
  await page.waitForFunction(() => document.querySelector('.agent-row.own > .numeric:not(.cost-cell) .animated-value')?.getAttribute('aria-label') === '7.10M');
  await page.evaluate(() => new Promise(requestAnimationFrame));
  const afterInterrupt = parseFloat(await ownValue.innerText());
  assert.ok(afterInterrupt >= beforeInterrupt && afterInterrupt < 6.2, `Continue from the visible number, without jumping to the previous target: ${beforeInterrupt} -> ${afterInterrupt}`);
  await waitValues({ ownTokens: '7.10M', detailTokens: '7.10M' });
  check('A second update during the 450ms transition starts from the number currently on screen');

  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: '按会话', exact: true }).click();
  await page.locator('.sessions-table').waitFor();
  const sessionTargets = {
    mainTokens: valueText('.sessions-table [data-uid="main"] > .numeric:not(.cost-cell)'),
    mainCost: valueText('.sessions-table [data-uid="main"] > .cost-cell'),
    childTokens: valueText('.sessions-table [data-uid="agent-a"] > .numeric:not(.cost-cell)'),
  };
  await monitor(sessionTargets);
  main.total_token_usage = usage(7600000); main.estimated_cost_usd = 7.48;
  await publish();
  await page.waitForFunction(targets => document.querySelector(targets.mainTokens)?.textContent === '7.60M' && document.querySelector(targets.mainCost)?.textContent === '$7.48', sessionTargets);
  const overview = await trace();
  assert.ok(distinct(overview, 'mainTokens').length > 3);
  assert.ok(distinct(overview, 'mainCost').length > 3);
  assert.equal(distinct(overview, 'childTokens').length, 1);
  await page.screenshot({ path: `${output}/overview.png` });
  check('The overview session list animates only the changed row values');

  await page.emulateMedia({ reducedMotion: 'reduce' });
  main.total_token_usage = usage(9400000);
  await publish();
  await page.waitForFunction(() => document.querySelector('.sessions-table [data-uid="main"] .animated-value')?.getAttribute('aria-label') === '9.40M');
  assert.equal(await page.locator(sessionTargets.mainTokens).innerText(), '9.40M');
  main.estimated_cost_usd = null;
  await publish();
  await page.waitForFunction(() => document.querySelector('.sessions-table [data-uid="main"] > .cost-cell')?.textContent.includes('未知'));
  assert.equal((await page.locator('.detail-cost strong').innerText()).trim(), '未知');
  main.estimated_cost_usd = 7.48;
  await publish();
  await page.waitForFunction(() => document.querySelector('.sessions-table [data-uid="main"] > .cost-cell .animated-value')?.getAttribute('aria-label') === '$7.48');
  assert.equal(await page.locator(sessionTargets.mainCost).innerText(), '$7.48');
  check('Reduced motion updates immediately, and unknown costs keep their meaning when values become available again');
  assert.deepEqual(report.errors, []);
  report.passed = true;
} catch (error) {
  report.failure = String(error);
  report.failureValues = await page.evaluate(targets => Object.fromEntries(Object.entries(targets).map(([key, selector]) => [key, document.querySelector(selector)?.textContent])), targets);
  await page.screenshot({ path: `${output}/failure.png` });
  throw error;
} finally {
  await writeFile(`${output}/checks.json`, JSON.stringify(report, null, 2));
  await browser.close();
}
