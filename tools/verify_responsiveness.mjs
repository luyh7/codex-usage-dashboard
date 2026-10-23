import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture, session, summary, stamp } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const base = process.env.DASHBOARD_URL || 'http://127.0.0.1:8791';
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH, headless: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 900 }, locale: 'zh-CN', reducedMotion: 'reduce' });
await installMintFixture(context);
await context.addInitScript(() => {
  localStorage.setItem('cousash:v2:projects:this-device', JSON.stringify(Array.from({ length: 1632 }, (_, i) => `project-${i}`)));
  window.EventSource = class extends EventTarget {
    constructor() { super(); window.__stream = this; }
    close() {}
  };
});
const rows = Array.from({ length: 1632 }, (_, i) => session(`row-${i}`, `Task ${i}`, 1000 + i, 1, `project-${i}`));
const totals = summary(rows);
const projects = rows.map(row => ({ key: row.project, label: row.project, ...summary([row]), updated_at: stamp, environments: ['Local'] }));
const requests = [];
let revision = 1, delay = 0, fail = false, failMore = false, staleMore = false;
await context.route('**/api/v2/project*', async route => {
  const url = new URL(route.request().url()), p = url.searchParams;
  const request = { path: url.pathname, search: p.get('search') || '', cursor: p.get('cursor'), project: p.get('project') };
  requests.push(request);
  if (request.cursor && staleMore) {
    staleMore = false;
    return route.fulfill({ status: 409, json: { error: 'snapshot stale' } });
  }
  if (request.cursor && failMore) return route.fulfill({ status: 503, json: { error: 'more unavailable' } });
  const search = request.search;
  const filtered = projects.filter(row => !search || row.key.includes(search));
  const isList = url.pathname.endsWith('/projects');
  const row = rows.find(row => row.project === request.project);
  const all = isList ? filtered : row ? [{ identity: row.uid, sessions: [row], summary: summary([row]) }] : [];
  const offset = Number(p.get('cursor') || p.get('offset') || 0), limit = Number(p.get('limit') || 30);
  const selectedRevision = `responsive-${revision}`;
  const page = { query_token: `${selectedRevision}:${search}`, revision: selectedRevision, items: all.slice(offset, offset + limit), summary: search ? summary(rows.filter(row => row.project.includes(search))) : totals,
    total: all.length, offset, limit, next_cursor: offset + limit < all.length ? String(offset + limit) : null, previous_cursor: offset ? String(Math.max(0, offset - limit)) : null };
  const body = isList ? { page, period: { key: p.get('period'), start_date: '2026-10-19', end_date: '2026-10-25' }, facets: totals, generated_at: stamp, codex_sources: [], remotes: [], current_device_short_code: 'this-device' } : page;
  if (isList && delay) await new Promise(resolve => setTimeout(resolve, delay));
  if (isList && fail) return route.fulfill({ status: 503, json: { error: 'fixture unavailable' } }).catch(() => {});
  await route.fulfill({ json: body }).catch(() => {});
});
const page = await context.newPage();
page.setDefaultTimeout(8000);
const errors = [], failures = [], checks = [];
page.on('pageerror', error => errors.push(String(error)));
const check = (label, test) => { try { test(); checks.push(label); } catch (e) { failures.push(`${label}: ${e.message}`); } };
try {
  await page.goto(base + '/?period=all&timezone=Asia%2FShanghai');
  await page.locator('.task-row').first().waitFor();
  const firstPaint = { projects: await page.locator('.project-block').count(), listings: requests.filter(r => r.path.endsWith('/projects')).length, groups: requests.filter(r => r.project).length };
  console.log(JSON.stringify({ firstPaint, errors }));
  check('bounded first-screen work for 1,632 projects', () => { assert.ok(firstPaint.projects <= 30, JSON.stringify(firstPaint)); assert.ok(firstPaint.listings <= 2, JSON.stringify(firstPaint)); assert.ok(firstPaint.groups <= 20, JSON.stringify(firstPaint)); });
  await page.locator('.browser-scroll').evaluate(el => { el.scrollTop = 80; window.__savedScroll = el.scrollTop; window.__savedBrowser = el; window.__savedRow = el.querySelector('.task-row'); });
  const beforeNavigation = requests.length;
  await page.locator('.main-nav button').nth(1).evaluate(el => el.click());
  await page.locator('.settings-header').waitFor();
  const navigationMs = await page.evaluate(async () => { const start = performance.now(); document.querySelector('.main-nav button').click(); await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame); return performance.now() - start; });
  await page.locator('.browser-scroll').waitFor({ state: 'visible' });
  check('settings return reuses DOM and scroll without refetching', () => assert.equal(requests.length, beforeNavigation));
  const retained = await page.evaluate(() => document.querySelector('.browser-scroll') === window.__savedBrowser && window.__savedRow?.isConnected && Math.abs(document.querySelector('.browser-scroll').scrollTop - window.__savedScroll) < 2);
  check('settings return retains DOM identity and scroll', () => assert.ok(retained));
  console.log(JSON.stringify({ firstPaint, navigationMs, checks, failures }));
  if (process.env.BASELINE) { assert.deepEqual(failures, []); } else {
    // Only the newest search may commit. Previous results stay visible while pending.
    delay = 450;
    const oldTitle = await page.locator('.project-heading').first().innerText();
    await page.getByRole('searchbox').fill('project-1631');
    await page.locator('.results-progress').waitFor();
    assert.equal(await page.locator('.project-heading').first().innerText(), oldTitle);
    await page.waitForFunction(() => document.querySelector('.browser-pane').getAttribute('aria-busy') === 'false');
    await page.waitForFunction(() => document.querySelector('.project-heading')?.textContent.includes('project-1631'));
    assert.equal(await page.locator('.project-block').count(), 1);
    checks.push('delayed search keeps previous results with progress, then commits the final result');
    const beforeSearch = requests.filter(r => r.path.endsWith('/projects')).length;
    await page.getByRole('searchbox').fill('p');
    await page.getByRole('searchbox').fill('project-');
    await page.getByRole('searchbox').fill('project-1630');
    await page.waitForFunction(() => document.querySelector('.project-heading')?.textContent.includes('project-1630'));
    assert.equal(requests.filter(r => r.path.endsWith('/projects')).length - beforeSearch, 1);
    checks.push('rapid typing makes one request');
    const obsolete = page.waitForRequest(r => new URL(r.url()).searchParams.get('search') === 'project-1628');
    await page.getByRole('searchbox').fill('project-1628');
    await obsolete;
    await page.getByRole('searchbox').fill('project-1627');
    await page.waitForFunction(() => document.querySelector('.project-heading')?.textContent.includes('project-1627'));
    await page.waitForTimeout(500);
    assert.match(await page.locator('.project-heading').first().innerText(), /project-1627/);
    checks.push('superseded slow responses cannot replace newer results');
    fail = true;
    await page.getByRole('searchbox').fill('project-1629');
    await page.getByRole('alert').waitFor();
    assert.match(await page.locator('.project-heading').first().innerText(), /project-1627/);
    assert.equal(await page.locator('.results-progress').count(), 0);
    fail = false; delay = 0;
    await page.getByRole('button', { name: '重试', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.project-heading')?.textContent.includes('project-1629'));
    checks.push('failed filtering retains results and retries');
    await page.getByRole('searchbox').fill('no-such-project');
    await page.getByText('没有符合条件的会话', { exact: true }).waitFor();
    assert.equal(await page.locator('.project-block').count(), 0);
    checks.push('empty filters replace the previous results');
    failMore = true;
    await page.getByRole('searchbox').fill('');
    await page.waitForFunction(() => document.querySelector('.project-heading')?.textContent.includes('project-0'));
    await page.locator('.browser-scroll').evaluate(el => el.scrollTop = el.scrollHeight);
    await page.getByRole('alert').filter({ hasText: 'more unavailable' }).waitFor();
    failMore = false;
    staleMore = true;
    await page.getByRole('button', { name: '重试', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.browser-pane').getAttribute('aria-busy') === 'false' && !document.querySelector('[role="alert"]'));
    checks.push('pagination failure retries and stale cursors recover');
    // Traverse every server page, without keeping every project mounted.
    for (let i = 0; i < 65; i++) {
      await page.locator('.browser-scroll').evaluate(el => el.scrollTop = el.scrollHeight);
      await page.waitForTimeout(35);
      assert.ok(await page.locator('.project-block').count() <= 30);
      if (await page.locator('.project-heading').filter({ hasText: /^project-1631/ }).count()) break;
    }
    await page.locator('.project-heading').filter({ hasText: /^project-1631/ }).waitFor();
    checks.push('all 1,632 projects are reachable with bounded DOM');
    // Recently visited virtual rows reuse their task data on remount.
    await page.locator('.task-row').last().waitFor();
    const cachedKey = await page.locator('.virtual-project').last().getAttribute('data-project-key');
    const cachedRequests = requests.filter(r => r.project === cachedKey).length;
    const end = await page.locator('.browser-scroll').evaluate(el => el.scrollTop);
    await page.locator('.browser-scroll').evaluate(el => el.scrollTop -= 2500);
    await page.waitForTimeout(100);
    await page.locator('.browser-scroll').evaluate((el, top) => el.scrollTop = top, end);
    await page.locator('.project-heading').filter({ hasText: /^project-1631/ }).waitFor();
    assert.equal(requests.filter(r => r.project === cachedKey).length, cachedRequests);
    checks.push('recent virtual rows reuse loaded task data');
    await page.locator('.browser-scroll').evaluate(el => el.scrollTop = 0);
    await page.locator('.task-row').first().waitFor();
    const beforeRefresh = await page.locator('.browser-scroll').evaluate(el => { el.scrollTop = 70; return el.scrollTop; });
    revision++;
    await page.evaluate(revision => window.__stream.dispatchEvent(new MessageEvent('revision', { data: JSON.stringify({ phase: 'ready', revision }) })), revision);
    await page.waitForResponse(r => r.url().includes('/api/v2/projects?'));
    await page.waitForFunction(() => document.querySelector('.browser-pane').getAttribute('aria-busy') === 'false');
    assert.ok(Math.abs((await page.locator('.browser-scroll').evaluate(el => el.scrollTop)) - beforeRefresh) < 3);
    checks.push('revision refresh preserves scroll');
    await page.setViewportSize({ width: 390, height: 844 });
    await page.locator('.task-row').first().waitFor();
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    checks.push('virtual rows resize without mobile overflow');
    assert.deepEqual(failures, []);
  }
  assert.deepEqual(errors, []);
  await mkdir('output/verification/responsiveness', { recursive: true });
  await page.screenshot({ path: 'output/verification/responsiveness/dashboard.png' });
  await writeFile('output/verification/responsiveness/report.json', JSON.stringify({ firstPaint, navigationMs, checks, failures, errors }, null, 2));
  console.log(JSON.stringify({ checks, failures, errors }));
} finally { await browser.close(); }
