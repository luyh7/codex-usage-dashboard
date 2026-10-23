import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const origin = process.env.DASHBOARD_URL || 'http://127.0.0.1:8765';
const output = 'output/verification/detail-filters';
await mkdir(output, { recursive: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 1024 }, locale: 'zh-CN', reducedMotion: 'reduce' });
await installMintFixture(context);
await context.addInitScript(() => {
  window.EventSource = class extends EventTarget {
    constructor() { super(); window.__revisionStream = this; }
    close() {}
  };
});
const page = await context.newPage();
page.setDefaultTimeout(8000);
const report = { checks: [], errors: [], port: 8765 };
const check = label => { report.checks.push(label); console.log(label); };
page.on('pageerror', error => report.errors.push(String(error)));
const detail = page.locator('.detail-panel');
const selected = () => new URL(page.url()).searchParams.get('session');
const ready = () => page.waitForFunction(() => document.querySelector('.detail-panel')?.getAttribute('aria-busy') === 'false');
const navigate = async (extra = '') => {
  await page.goto(`${origin}/?view=sessions&period=week${extra}`);
  await page.locator('.sessions-table [data-uid="main"]').waitFor();
};
const open = async () => {
  await page.locator('.sessions-table [data-uid="main"]').click();
  await ready();
  assert.equal(selected(), 'main');
};
const search = async value => {
  await page.getByRole('searchbox').fill(value);
  await page.waitForFunction(value => new URL(location.href).searchParams.get('search') === (value || null), value);
};
let responseMode = '', release, started, holdRefresh = false, refreshStarted, releaseRefresh;
await context.route('**/api/v2/sessions?*', async route => {
  const params = new URL(route.request().url()).searchParams;
  if (holdRefresh && params.has('if_query_token')) {
    holdRefresh = false;
    refreshStarted();
    await new Promise(resolve => { releaseRefresh = resolve; });
    return route.fulfill({ status: 204 });
  }
  return route.fallback();
});
await context.route('**/api/v2/session?*', async route => {
  if (responseMode === 'missing') return route.fulfill({ status: 404, contentType: 'application/json', body: JSON.stringify({ error: 'missing', code: 'details_missing' }) });
  if (responseMode === 'failure') return route.fulfill({ status: 500, contentType: 'application/json', body: JSON.stringify({ error: 'temporary failure' }) });
  if (responseMode === 'delay') {
    responseMode = '';
    started?.();
    await new Promise(resolve => { release = resolve; });
    return route.fulfill({ status: 404, contentType: 'application/json', body: JSON.stringify({ error: 'old failure', code: 'details_missing' }) }).catch(() => {});
  }
  return route.fallback();
});

try {
  // Real backend checks use read-only requests against the current worktree.
  const health = await (await context.request.get(`${origin}/api/health`)).json();
  report.build_id = health.build_id;
  assert.equal(health.app, 'codex-usage-dashboard');
  const listing = await (await context.request.get(`${origin}/api/v2/sessions?period=all&limit=1`)).json();
  const row = listing.page.items[0];
  assert.ok(row, 'At least one local session is required for HTTP selection checks');
  for (const filter of [{ search: 'no-session-could-match-this-70a8c39c' }, { source: 'no-source' }, { model: 'no-model' }, { environment: 'no-device' }, { agents: row.is_subagent ? 'main' : 'agents' }]) {
    const params = new URLSearchParams({ ...filter, id: row.uid, snapshot_token: listing.page.revision, check_filters: 'true' });
    const response = await context.request.get(`${origin}/api/v2/session?${params}`);
    assert.equal(response.status(), 404);
    assert.equal((await response.json()).code, 'session_filtered_out');
  }
  const historical = await (await context.request.get(`${origin}/api/v2/sessions?period=custom&start=2010-01-01&end=2010-01-02`)).json();
  const params = new URLSearchParams({ id: row.uid, snapshot_token: historical.page.revision, check_filters: 'true' });
  assert.equal((await (await context.request.get(`${origin}/api/v2/session?${params}`)).json()).code, 'session_filtered_out');
  check('Real HTTP selection checks distinguish exclusion by search, status, model, device, agent role and date range');

  await navigate();
  holdRefresh = true;
  const refreshing = new Promise(resolve => { refreshStarted = resolve; });
  await page.evaluate(() => window.__revisionStream.dispatchEvent(new MessageEvent('revision', { data: JSON.stringify({ phase: 'ready', revision: 2 }) })));
  await refreshing;
  await page.locator('.sessions-table [data-uid="main"]').click();
  releaseRefresh();
  await ready();
  assert.equal(selected(), 'main');
  assert.equal(await detail.locator('.event-scroll tbody tr').count(), 100);
  check('Selecting during an unchanged background refresh still loads details after HTTP 204');

  await navigate();
  await open();
  await search('登录');
  await ready();
  assert.equal(selected(), 'main');
  assert.equal(await detail.locator('[role="alert"]').count(), 0);
  check('Matching search retains the selected session even when the revision is unchanged');
  await search('指南');
  await detail.waitFor({ state: 'detached' });
  assert.equal(selected(), null);
  assert.equal(await page.locator('[role="alert"]').count(), 0);
  assert.equal(await page.locator('.selected[data-uid]').count(), 0);
  await page.goBack();
  await ready();
  assert.equal(selected(), 'main');
  assert.equal(new URL(page.url()).searchParams.get('search'), null);
  check('Excluding search clears selection and URL without an error, replacement selection or extra history entry; Back restores the original selection');

  await navigate();
  await open();
  await page.locator('.filter-toggle').click();
  const filters = page.getByRole('dialog', { name: '筛选', exact: true });
  await filters.getByRole('group', { name: '状态', exact: true }).getByRole('button', { name: '已归档', exact: true }).click();
  await filters.getByRole('button', { name: '确定', exact: true }).click();
  await detail.waitFor({ state: 'detached' });
  assert.equal(selected(), null);
  check('Applying global filters silently closes an excluded session');

  await navigate();
  await open();
  await page.locator('.calendar-trigger').click();
  await page.getByLabel('开始日期', { exact: true }).fill('2010-01-01');
  await page.getByLabel('结束日期', { exact: true }).fill('2010-01-02');
  await page.locator('.calendar-panel').getByRole('button', { name: '应用日期', exact: true }).click();
  await detail.waitFor({ state: 'detached' });
  assert.equal(selected(), null);
  assert.equal(await page.locator('[role="alert"]').count(), 0);
  check('Changing the calendar range clears a session with no usage in the new range');

  await navigate();
  responseMode = 'delay';
  const pending = new Promise(resolve => { started = resolve; });
  await page.locator('.sessions-table [data-uid="main"]').click();
  await pending;
  await search('登录');
  await ready();
  release();
  await page.waitForTimeout(150);
  assert.equal(selected(), 'main');
  assert.equal(await detail.locator('[role="alert"]').count(), 0);
  check('A delayed old detail failure cannot overwrite the same session under a newer filter');

  await navigate();
  responseMode = 'missing';
  await open();
  assert.match(await detail.getByRole('alert').innerText(), /此快照未包含会话详情/);
  assert.equal(await detail.getByRole('button', { name: '重试', exact: true }).count(), 0);
  responseMode = 'failure';
  await search('登录');
  await ready();
  assert.equal(await detail.getByRole('button', { name: '重试', exact: true }).count(), 1);
  responseMode = '';
  await detail.getByRole('button', { name: '重试', exact: true }).click();
  await ready();
  assert.equal(await detail.getByRole('alert').count(), 0);
  check('Missing snapshot details have no retry button; transient errors remain retryable and recover');

  assert.deepEqual(report.errors, []);
  report.passed = true;
  await page.screenshot({ path: `${output}/retained-detail.png` });
  await writeFile(`${output}/checks.json`, JSON.stringify(report, null, 2));
} catch (error) {
  await page.screenshot({ path: `${output}/failure.png` });
  await writeFile(`${output}/failure.txt`, `${error}\n${await page.locator('body').innerText()}`);
  throw error;
} finally {
  await browser.close();
}
