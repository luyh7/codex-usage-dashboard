// Exercise project pagination against the real server and its bound cursors.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { once } from 'node:events';
import { mkdir, mkdtemp, rename, writeFile } from 'node:fs/promises';
import path from 'node:path';

const root = process.cwd();
const output = path.join(root, 'output/verification/project-pagination');
await mkdir(output, { recursive: true });
const fixture = await mkdtemp(path.join(output, 'run-'));
const config = path.join(fixture, 'state');
const home = path.join(fixture, 'codex');
const sessions = path.join(home, 'sessions');
await mkdir(sessions, { recursive: true });
await mkdir(config, { recursive: true });
await writeFile(path.join(config, 'preferences.json'), JSON.stringify({ schema_version: 1, language: 'zh', timezone: 'Asia/Shanghai', codex_homes: [] }));

async function writeProject(dir, number) {
  const name = `cursor-project-${String(number).padStart(3, '0')}`;
  const rows = [
    { timestamp: '2026-10-01T00:00:00Z', type: 'session_meta', payload: { id: name, model: 'gpt-5', cwd: '/synthetic/' + name } },
    { timestamp: '2026-10-01T00:00:01Z', type: 'event_msg', payload: { type: 'token_count', info: { total_token_usage: { input_tokens: 100, total_tokens: 100 } } } },
  ];
  await writeFile(path.join(dir, `rollout-${name}.jsonl`), rows.map(JSON.stringify).join('\n') + '\n');
}
await writeProject(sessions, 0);

const binary = path.join(root, 'target', 'debug', 'codex-usage-dashboard' + (process.platform === 'win32' ? '.exe' : ''));
const shutdownToken = randomBytes(32).toString('hex');
const child = spawn(binary, ['--serve', '--managed', '--port', '0', '--codex-home', home, '--no-auto-windows'], {
  cwd: root, windowsHide: true,
  env: { ...process.env, COUSASH_CONFIG_DIR: config, COUSASH_INSTANCE_ID: randomBytes(32).toString('hex'), COUSASH_SHUTDOWN_TOKEN: shutdownToken },
  stdio: ['ignore', 'pipe', 'pipe'],
});
let stdout = '', stderr = '', base, browser;
child.stdout.on('data', chunk => stdout += chunk);
child.stderr.on('data', chunk => stderr += chunk);
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(fn, timeout = 30000) {
  const deadline = Date.now() + timeout;
  let last;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Dashboard exited: ${stderr}`);
    try { const value = await fn(); if (value) return value; } catch (error) { last = error; }
    await pause(50);
  }
  throw new Error(`Timed out: ${last || 'condition not met'}`);
}
const json = async route => (await fetch(base + route)).json();
async function refresh() {
  const token = (await json('/api/security/token')).token;
  const response = await fetch(base + '/api/refresh', { method: 'POST', headers: { 'x-codex-write-token': token, 'Content-Type': 'application/json' } });
  assert.equal(response.status, 200, await response.text());
}
const isProjectList = response => new URL(response.url()).pathname === '/api/v2/projects';
const limitOf = response => Number(new URL(response.url()).searchParams.get('limit'));
const hasCursor = response => !!new URL(response.url()).searchParams.get('cursor');

try {
  base = await until(() => stdout.match(/http:\/\/127\.0\.0\.1:\d+/)?.[0]);
  await until(async () => (await json('/api/health')).index.phase === 'ready');
  const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
  browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
  const page = await browser.newPage({ viewport: { width: 1536, height: 900 }, locale: 'zh-CN', reducedMotion: 'reduce' });
  page.setDefaultTimeout(30000);
  const watch = predicate => {
    const response = page.waitForResponse(predicate);
    response.catch(() => {}); // Preserve the original failure if cleanup closes the page first.
    return response;
  };
  const errors = [], responses = [];
  page.on('pageerror', error => errors.push(String(error)));
  page.on('response', response => { if (isProjectList(response)) responses.push(response); });
  await page.goto(base + '/?period=all&timezone=Asia%2FShanghai');
  await page.locator('.project-heading').filter({ hasText: 'cursor-project-000' }).waitFor();

  // A small listing grows during live refresh; its next cursor is bound to limit 1.
  const nextAfterGrowth = watch(response => isProjectList(response) && hasCursor(response));
  await writeProject(sessions, 1);
  await refresh();
  const next = await nextAfterGrowth;
  assert.equal(next.status(), 200, await next.text());
  assert.equal(limitOf(next), 1);
  await page.locator('.project-heading').filter({ hasText: 'cursor-project-001' }).waitFor();
  assert.equal(await page.getByRole('alert').count(), 0);

  // Prepare enough projects to exercise non-default and multi-page refreshes.
  const staged = path.join(fixture, 'staged');
  await mkdir(staged);
  for (let i = 2; i < 212; i++) await writeProject(staged, i);
  await rename(staged, path.join(sessions, 'batch'));
  await refresh();
  await until(async () => (await json('/api/v2/projects?period=all&timezone=Asia%2FShanghai&limit=1')).page?.total === 212);
  await page.reload();
  await page.locator('.project-heading').first().waitFor();
  await until(() => responses.some(response => isProjectList(response) && !hasCursor(response) && limitOf(response) === 30 && response.status() === 200));

  // Load two initial pages, then grow the index. The refreshed cursor uses 60.
  const secondPage = watch(response => isProjectList(response) && hasCursor(response) && limitOf(response) === 30);
  await page.locator('.browser-scroll').evaluate(element => { element.scrollTop = element.scrollHeight; });
  assert.equal((await secondPage).status(), 200);
  await pause(250);
  const refreshed = watch(response => isProjectList(response) && !hasCursor(response) && limitOf(response) === 60);
  await writeProject(sessions, 212);
  await refresh();
  assert.equal((await refreshed).status(), 200);
  const nextAtSixty = watch(response => isProjectList(response) && hasCursor(response));
  await page.locator('.browser-scroll').evaluate(element => { element.scrollTop = element.scrollHeight; });
  const sixty = await nextAtSixty;
  assert.equal(sixty.status(), 200, await sixty.text());
  assert.equal(limitOf(sixty), 60);
  await pause(250);

  // Refreshing more than 100 loaded rows re-fetches pages using the same limit.
  const refreshAtHundred = watch(response => isProjectList(response) && !hasCursor(response) && limitOf(response) === 100);
  const restoreAtHundred = watch(response => isProjectList(response) && hasCursor(response) && limitOf(response) === 100);
  await writeProject(sessions, 213);
  await refresh();
  assert.equal((await refreshAtHundred).status(), 200);
  assert.equal((await restoreAtHundred).status(), 200);
  await pause(250);
  const finalPage = watch(response => isProjectList(response) && hasCursor(response) && limitOf(response) === 100);
  await page.locator('.browser-scroll').evaluate(element => { element.scrollTop = element.scrollHeight; });
  assert.equal((await finalPage).status(), 200);
  assert.equal(await page.getByRole('alert').count(), 0);
  assert.deepEqual(errors, []);
  console.log('PASS: real server pagination after project growth, 60-row refresh, and multi-page refresh');
} finally {
  if (browser) await browser.close();
  if (child.exitCode === null) {
    const exited = once(child, 'exit');
    if (base) await fetch(base + '/api/shutdown', { method: 'POST', headers: { Authorization: 'Bearer ' + shutdownToken } }).catch(() => {});
    else child.kill();
    await Promise.race([exited, pause(5000)]);
    if (child.exitCode === null) child.kill();
  }
}
