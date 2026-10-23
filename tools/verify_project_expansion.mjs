import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture, session, sessions, summary, stamp } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const output = 'output/verification/project-expansion';
await mkdir(output, { recursive: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 800 }, locale: 'zh-CN', reducedMotion: 'reduce' });
await installMintFixture(context);
const keys = ['web-app', 'data-tools', ...Array.from({ length: 31 }, (_, i) => `project-${i}`)];
const rows = keys.flatMap((key, project) => Array.from({ length: project === 0 ? 205 : project === 1 ? 5 : 1 }, (_, i) =>
  project === 0 && i === 0 ? sessions[0] : session(`${key}-${i}`, `${key} task ${i}`, 1000, 1, key)));
rows.push(sessions[1], sessions[2]);
const requests = [];
let currentRevision = 'project-expansion-v1';
const groups = key => rows.filter(row => row.project === key && !row.is_subagent).map(root => {
  const members = rows.filter(row => row.uid === root.uid || row.parent_thread_id === root.uid);
  return { identity: root.uid, sessions: members, summary: summary(members) };
});
const projects = keys.map(key => ({ key, label: key, ...summary(rows.filter(row => row.project === key)), environments: ['Local'] }));
await context.route('**/api/v2/project*', async route => {
  const url = new URL(route.request().url()), p = url.searchParams;
  requests.push({ path: url.pathname, project: p.get('project'), cursor: p.get('cursor'), limit: Number(p.get('limit')), revision: p.get('revision') });
  const all = url.pathname.endsWith('/projects') ? projects : groups(p.get('project'));
  const offset = Number(p.get('cursor') || p.get('offset') || 0), limit = Number(p.get('limit'));
  const page = { query_token: currentRevision, revision: currentRevision, items: all.slice(offset, offset + limit), summary: summary(rows), total: all.length, offset, limit,
    next_cursor: offset + limit < all.length ? String(offset + limit) : null, previous_cursor: offset ? String(Math.max(0, offset - limit)) : null };
  const body = url.pathname.endsWith('/projects') ? { page, facets: summary(rows), metric_trend: { complete: true, points: [] },
    period: { key: 'all', start_date: '2026-10-19', end_date: '2026-10-25' }, generated_at: stamp, codex_sources: [], remotes: [], current_device_short_code: 'this-device' } : page;
  await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
});
const page = await context.newPage();
page.setDefaultTimeout(10000);
const errors = [];
page.on('pageerror', error => errors.push(String(error)));
const first = page.locator('.project-block').nth(0);
const second = page.locator('.project-block').nth(1);
const checkCount = (block, count) => page.waitForFunction(({ index, count }) => document.querySelectorAll('.project-block')[index]?.querySelectorAll('.task-group').length === count, { index: block, count });
const pinned = async block => {
  const delta = await block.locator('.project-heading').evaluate(el => el.getBoundingClientRect().top - el.closest('.browser-scroll').getBoundingClientRect().top - 31);
  assert.ok(Math.abs(delta) < 2, `Project heading sticks below columns: ${delta}`);
};
try {
  await page.goto('http://127.0.0.1:8765/?period=all&timezone=Asia%2FShanghai');
  await checkCount(0, 5);
  assert.equal(await page.locator('.project-block').count(), 33, 'Every project is present beyond the old 30-project page');
  assert.equal(await page.locator('.pager').count(), 0);
  assert.equal(await first.locator('.agent-row').count(), 3, 'Subagents stay attached to their main task and do not consume preview slots');
  assert.match(await first.locator('.project-more').innerText(), /200/);
  assert.equal(requests.find(r => r.path.endsWith('/project-tasks')).limit, 5);
  await first.locator('.project-more').click();
  await checkCount(0, 205);
  assert.equal(await first.locator('.project-more').innerText(), '收起到 5 个会话组');
  assert.ok(requests.some(r => r.cursor === '200' && r.project === 'web-app'), 'Expansion includes every server page');
  assert.ok(requests.filter(r => r.project === 'web-app' && r.cursor).every(r => r.revision === currentRevision));
  await page.locator('.browser-scroll').evaluate(el => el.scrollTop = 500);
  await pinned(first);
  await page.screenshot({ path: `${output}/sticky-expanded.png` });

  await second.locator('.project-heading').click();
  await checkCount(1, 5);
  assert.equal(await second.locator('.project-more').count(), 0, 'Exactly five tasks need no expansion control');
  await page.locator('.browser-scroll').evaluate(el => {
    const heading = el.querySelectorAll('.project-heading')[1];
    el.scrollTop += heading.getBoundingClientRect().top - el.getBoundingClientRect().top - 31 + 25;
  });
  await pinned(second);
  assert.ok((await first.locator('.project-heading').boundingBox()).y < (await second.locator('.project-heading').boundingBox()).y);

  await first.locator('.project-more').click();
  await checkCount(0, 5);
  assert.equal(await first.locator('.project-more').getAttribute('aria-expanded'), 'false');
  const tasksBeforeReopen = requests.filter(r => r.project === 'web-app').length;
  await first.locator('.project-more').click();
  await checkCount(0, 205);
  assert.equal(requests.filter(r => r.project === 'web-app').length, tasksBeforeReopen, 'Reopening uses the full loaded list');

  currentRevision = 'project-expansion-v2';
  const refreshed = page.waitForResponse(response => {
    const url = new URL(response.url());
    return url.searchParams.get('revision') === currentRevision && url.searchParams.get('project') === 'web-app' && url.searchParams.get('cursor') === '200';
  });
  await page.clock.install();
  await page.clock.runFor(15010);
  await refreshed;
  await page.waitForFunction(() => document.querySelector('.project-more')?.textContent === '收起到 5 个会话组');
  await checkCount(0, 205);
  assert.ok(requests.some(r => r.project === 'web-app' && r.revision === currentRevision && r.cursor === '200'), 'Live refresh preserves expanded state and reloads every page');
  await page.clock.resume();

  for (const width of [390, 320]) {
    await page.setViewportSize({ width, height: 844 });
    await page.locator('.browser-scroll').evaluate(el => el.scrollTop = 400);
    await pinned(first);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    await first.locator('.project-more').click();
    await checkCount(0, 5);
    assert.ok((await first.locator('.project-heading').boundingBox()).y >= (await page.locator('.browser-scroll').boundingBox()).y, 'Collapsing keeps the current project visible');
    await page.screenshot({ path: `${output}/collapsed-${width}.png` });
    await first.locator('.project-more').click();
    await checkCount(0, 205);
  }
  await page.getByRole('searchbox').fill('web-app');
  await checkCount(0, 5);
  assert.equal(await first.locator('.project-more').getAttribute('aria-expanded'), 'false', 'A new filter starts with five tasks');
  assert.deepEqual(errors, []);
  await writeFile(`${output}/checks.json`, JSON.stringify({ passed: true, projects: 33, expandedTasks: 205, viewports: [1536, 390, 320], errors, requests }, null, 2));
  console.log('PASS: all 33 projects, five-task preview, 205-task expansion, collapse/reopen, live refresh, subagents and desktop/mobile sticky headings');
} catch (error) {
  await page.screenshot({ path: `${output}/failure.png` });
  throw error;
} finally {
  await browser.close();
}
