import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture, sessions, summary, usage, stamp } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const output = 'output/verification/live-updates';
await mkdir(output, { recursive: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 1024 }, locale: 'zh-CN', timezoneId: 'Asia/Shanghai', reducedMotion: 'reduce' });
await installMintFixture(context);
// Drive the real subscription handler deterministically without changing any user logs.
await context.addInitScript(() => {
  const streams = new Set();
  window.EventSource = class extends EventTarget {
    constructor() { super(); streams.add(this); queueMicrotask(() => this.onopen?.(new Event('open'))); }
    close() { streams.delete(this); }
  };
  window.__publishIndexStatus = status => {
    for (const stream of streams) stream.dispatchEvent(new MessageEvent('revision', { data: JSON.stringify(status) }));
  };
});
let releaseListing;
const initialListing = new Promise(resolve => { releaseListing = resolve; });
let generation = 1, tokens = 2140000, dailyStatus = 200, holdDaily = true, dailyRequests = 0;
const pendingDaily = [];
const totals = summary(sessions);
await context.route('**/api/v2/sessions?*', async route => {
  await initialListing;
  const token = `query-${generation}`;
  if (new URL(route.request().url()).searchParams.get('if_query_token') === token) return route.fulfill({ status: 204 });
  await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({
    page: { query_token: token, revision: `live-${generation}`, items: sessions, summary: totals, total: sessions.length, offset: 0, limit: 100, next_cursor: null, previous_cursor: null },
    facets: totals, period: { key: 'week', start_date: '2026-10-19', end_date: '2026-10-25' }, generated_at: stamp, codex_sources: [], remotes: [], current_device_short_code: 'this-device',
  }) });
});
await context.route('**/api/daily-usage?*', async route => {
  dailyRequests++;
  const timezone = new URL(route.request().url()).searchParams.get('timezone');
  const status = dailyStatus;
  const body = JSON.stringify(status === 200 ? { generated_at: stamp, daily_usage_complete: true, daily_usage: [{ date: '2026-10-19', usage: usage(timezone === 'UTC' ? 1110000 : tokens) }] } : { error: 'Daily refresh failed in test' });
  if (holdDaily) await new Promise(resolve => pendingDaily.push(resolve));
  await route.fulfill({ status, contentType: 'application/json', body });
});
const report = { checks: [], errors: [] };
const base = process.env.DASHBOARD_URL || 'http://127.0.0.1:8765';
const page = await context.newPage();
page.setDefaultTimeout(8000);
page.on('pageerror', error => report.errors.push(String(error)));
await page.clock.setFixedTime(new Date('2026-10-22T04:00:00Z'));
const check = label => { report.checks.push(label); console.log(label); };
const emit = status => page.evaluate(status => window.__publishIndexStatus(status), status);
const waitPending = async count => {
  const deadline = Date.now() + 8000;
  while (pendingDaily.length < count) {
    assert.ok(Date.now() < deadline, `Waiting for ${count} daily requests`);
    await new Promise(resolve => setTimeout(resolve, 20));
  }
};
const day = () => page.locator('.calendar-grid [data-date="2026-10-19"] small');
const waitDay = text => page.waitForFunction(text => document.querySelector('.calendar-grid [data-date="2026-10-19"] small')?.textContent === text, text);
const statusText = () => page.locator('.index-status').innerText();
const draft = () => page.locator('.calendar-panel').evaluate(panel => ({
  from: panel.querySelector('[aria-label="开始日期"]').value,
  to: panel.querySelector('[aria-label="结束日期"]').value,
  month: {
    year: panel.querySelector('[aria-label="年"]').value,
    label: panel.querySelector('.calendar-month-dropdown summary').textContent,
  },
  focused: document.activeElement?.getAttribute('aria-label'),
  selectionStart: document.activeElement?.selectionStart,
  selectionEnd: document.activeElement?.selectionEnd,
}));
const changeZone = timezone => page.evaluate(timezone => {
  const url = new URL(location.href); url.searchParams.set('timezone', timezone);
  history.pushState(null, '', url); dispatchEvent(new PopStateEvent('popstate'));
}, timezone);
try {
  await page.goto(base + '/?view=sessions&period=week&timezone=Asia%2FShanghai');
  await page.locator('.index-waiting').waitFor();
  await emit({ phase: 'parsing', revision: 0, completed: 10, total: 128 });
  await page.waitForFunction(() => document.querySelector('.index-waiting')?.textContent.includes('10 / 128'));
  const progress = page.locator('.index-waiting [role=progressbar]');
  assert.equal(await progress.getAttribute('aria-valuenow'), '10');
  assert.equal(await progress.getAttribute('aria-valuemax'), '128');
  for (const phase of ['normalizing', 'committing']) {
    await emit({ phase, revision: 0, completed: 0, total: 128 });
    await page.waitForFunction(phase => document.querySelector('.index-waiting')?.textContent.includes(phase === 'normalizing' ? '正在整理会话' : '正在保存索引'), phase);
    assert.equal(await progress.getAttribute('aria-valuenow'), null, `${phase} must not show a fabricated percentage`);
  }
  releaseListing();
  await page.locator('.sessions-table').waitFor();
  await emit({ phase: 'ready', revision: 1 });
  assert.match(await statusText(), /^已更新\s+09:00:00$/);
  assert.equal(await page.locator('.index-status time').getAttribute('datetime'), stamp);
  await page.locator('.calendar-trigger').click();
  await waitPending(1);
  assert.equal(await day().innerText(), '…');
  pendingDaily.shift()();
  await waitDay('2.14M');
  check('First index and calendar loads still show progress until data becomes available');

  await page.locator('[data-date="2026-10-19"]').click();
  await page.locator('[data-date="2026-10-22"]').click();
  await page.getByLabel('结束日期', { exact: true }).focus();
  await page.getByLabel('结束日期', { exact: true }).evaluate(input => input.setSelectionRange(5, 7));
  const initialDraft = await draft();
  const initialUrl = page.url();
  await page.evaluate(() => {
    window.__originalCalendar = document.querySelector('.calendar-panel');
    window.__refreshFrames = [];
    new MutationObserver(() => window.__refreshFrames.push({
      status: document.querySelector('.index-status')?.textContent,
      day: document.querySelector('.calendar-grid [data-date="2026-10-19"] small')?.textContent,
      totals: document.querySelector('.calendar-totals')?.textContent,
    })).observe(document.querySelector('main'), { subtree: true, childList: true, characterData: true });
  });
  for (const phase of ['parsing', 'normalizing', 'committing']) {
    await emit({ phase, revision: 1, completed: 1, total: 128 });
    assert.match(await statusText(), /^已更新\s+09:00:00$/);
    assert.equal(await page.locator('.index-status').evaluate(el => el.classList.contains('working')), false);
  }
  generation = 2; tokens = 7330000;
  await emit({ phase: 'ready', revision: 2 });
  await waitPending(1);
  assert.equal(await day().innerText(), '2.14M', 'A delayed background refresh retains loaded daily values');
  assert.deepEqual(await draft(), initialDraft);
  pendingDaily.shift()();
  await waitDay('7.33M');
  assert.deepEqual(await draft(), initialDraft);
  assert.equal(page.url(), initialUrl);
  assert.equal(await page.evaluate(() => window.__originalCalendar === document.querySelector('.calendar-panel')), true);
  const frames = await page.evaluate(() => window.__refreshFrames);
  assert.ok(frames.length > 0);
  assert.ok(frames.every(frame => !frame.status.includes('建立索引') && frame.day !== '…' && !frame.totals.includes('—')));
  report.backgroundFrames = frames;
  await page.screenshot({ path: `${output}/calendar-background-refresh.png` });
  check('Background indexing and slow daily refreshes keep all values visible, preserve the calendar node, dates, month, focus and text selection');

  await page.keyboard.press('Escape');
  const callsBeforeClosedRefresh = dailyRequests;
  generation = 3; tokens = 8880000;
  await emit({ phase: 'ready', revision: 3 });
  await page.waitForFunction(() => document.querySelector('.browser-pane').getAttribute('aria-busy') === 'false');
  assert.equal(dailyRequests, callsBeforeClosedRefresh);
  await page.locator('.calendar-trigger').click();
  await waitPending(1);
  assert.equal(await day().innerText(), '7.33M');
  pendingDaily.shift()();
  await waitDay('8.88M');
  check('A closed calendar refreshes its stale cache when reopened and keeps the cached values visible meanwhile');

  generation = 4; tokens = 9990000; dailyStatus = 500;
  await emit({ phase: 'ready', revision: 4 });
  await waitPending(1);
  pendingDaily.shift()();
  await page.locator('.browser-scroll .error').filter({ hasText: 'Daily refresh failed in test' }).waitFor();
  assert.equal(await day().innerText(), '8.88M');
  await page.keyboard.press('Escape');
  dailyStatus = 200;
  await page.locator('.calendar-trigger').click();
  await waitPending(1);
  assert.equal(await day().innerText(), '8.88M');
  pendingDaily.shift()();
  await waitDay('9.99M');
  check('A failed refresh retains usable daily totals and retries when the calendar is reopened');

  await page.keyboard.press('Escape');
  await changeZone('UTC');
  await page.locator('.sessions-table').waitFor();
  await page.locator('.calendar-trigger').click();
  await waitPending(1);
  assert.equal(await day().innerText(), '…', 'Cached totals from another timezone must not be shown');
  await changeZone('Asia/Shanghai');
  await page.locator('.calendar-panel').waitFor({ state: 'detached' });
  await page.locator('.calendar-trigger').click();
  assert.equal(await day().innerText(), '9.99M');
  const lateResponse = page.waitForResponse(response => response.url().includes('/api/daily-usage?timezone=UTC'));
  pendingDaily.shift()();
  await lateResponse;
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  assert.equal(await day().innerText(), '9.99M', 'A late response for a superseded timezone cannot replace the current totals');
  await page.keyboard.press('Escape');
  await changeZone('UTC');
  await page.locator('.calendar-trigger').click();
  await waitPending(1);
  pendingDaily.shift()();
  await waitDay('1.11M');
  check('Timezone changes invalidate incompatible totals, and late responses cannot overwrite the active timezone');

  await emit({ phase: 'error', revision: 4, error: 'Index refresh failed in test' });
  await page.getByText('Index refresh failed in test', { exact: true }).waitFor();
  assert.equal(await statusText(), '索引暂时不可用');
  assert.equal(await day().innerText(), '1.11M');
  await emit({ phase: 'ready', revision: 4 });
  assert.match(await statusText(), /^已更新\s+01:00:00$/);
  check('Index errors remain visible and recovery clears the error state without discarding calendar data');
  assert.deepEqual(report.errors, []);
  report.passed = true;
} catch (error) {
  report.failure = String(error);
  await page.screenshot({ path: `${output}/failure.png` });
  throw error;
} finally {
  releaseListing();
  for (const release of pendingDaily) release();
  await writeFile(`${output}/checks.json`, JSON.stringify(report, null, 2));
  await browser.close();
}
