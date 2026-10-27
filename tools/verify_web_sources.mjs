import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { once } from 'node:events';
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { gunzipSync } from 'node:zlib';

const root = process.cwd();
const output = path.join(root, 'output/verification/web-sources');
await mkdir(output, { recursive: true });
const fixture = await mkdtemp(path.join(output, 'run-'));
const config = path.join(fixture, 'state');
const first = path.join(fixture, 'first');
const second = path.join(fixture, 'second');
for (const [home, id, total] of [[first, 'first', 12], [second, 'second', 24]]) {
  await mkdir(path.join(home, 'sessions'), { recursive: true });
  const rows = [
    { type: 'session_meta', payload: { id, model: 'gpt-5', cwd: '/synthetic/' + id } },
    { timestamp: '2026-10-26T00:00:00Z', type: 'event_msg', payload: { type: 'token_count', info: { total_token_usage: { input_tokens: total, total_tokens: total } } } },
  ];
  await writeFile(path.join(home, 'sessions', 'rollout-' + id + '.jsonl'), rows.map(JSON.stringify).join('\n') + '\n');
}
await mkdir(config, { recursive: true });
await writeFile(path.join(config, 'preferences.json'), JSON.stringify({ schema_version: 1, language: 'zh', timezone: 'UTC', codex_homes: [] }));
const binary = path.join(root, 'target', 'debug', 'codex-usage-dashboard' + (process.platform === 'win32' ? '.exe' : ''));
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const until = async (fn, timeout = 30000) => {
  const deadline = Date.now() + timeout;
  let last;
  while (Date.now() < deadline) {
    try { const value = await fn(); if (value) return value; } catch (error) { last = error; }
    await pause(50);
  }
  throw new Error('Timed out: ' + String(last || 'condition not met'));
};
let child, base, shutdownToken, browser;
async function start() {
  const instance = randomBytes(32).toString('hex');
  shutdownToken = randomBytes(32).toString('hex');
  child = spawn(binary, ['--serve', '--managed', '--port', '0', '--no-auto-windows', '--timezone', 'UTC'], {
    cwd: root, windowsHide: true, env: { ...process.env, COUSASH_CONFIG_DIR: config, CODEX_HOME: first, COUSASH_INSTANCE_ID: instance, COUSASH_SHUTDOWN_TOKEN: shutdownToken },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stdout = '', stderr = '';
  child.stdout.on('data', chunk => stdout += chunk);
  child.stderr.on('data', chunk => stderr += chunk);
  base = await until(() => {
    if (child.exitCode !== null) throw new Error('Service exited: ' + stderr);
    return stdout.match(/http:\/\/127\.0\.0\.1:\d+/)?.[0];
  });
  await until(async () => (await (await fetch(base + '/api/health')).json()).index.phase === 'ready');
}
async function stop() {
  if (!child || child.exitCode !== null) return;
  const exited = once(child, 'exit');
  await fetch(base + '/api/shutdown', { method: 'POST', headers: { Authorization: 'Bearer ' + shutdownToken } });
  let timer;
  try {
    await Promise.race([exited, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Service did not stop')), 10000); })]);
  } finally { clearTimeout(timer); }
}
const json = async path => (await fetch(base + path)).json();
const token = async () => (await json('/api/security/token')).token;
async function post(path, body, authorized = true) {
  return fetch(base + path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...(authorized ? { 'x-codex-write-token': await token() } : {}) },
    body: JSON.stringify(body),
  });
}
async function exportJob() {
  const created = await (await post('/api/export/start', {})).json();
  await until(async () => (await json('/api/export/progress?id=' + created.id)).phase === 'ready');
  const bytes = Buffer.from(await (await fetch(base + '/api/export.json.gz?job=' + created.id)).arrayBuffer());
  return { id: created.id, value: JSON.parse(gunzipSync(bytes).toString('utf8')) };
}
try {
  await start();
  const initial = (await json('/api/sources')).map(source => source.path);
  assert.equal(initial.length, 1);
  assert.equal((await post('/api/sources', { codex_homes: [], expected_homes: initial }, false)).status, 403);
  assert.equal((await post('/api/sources/choose', {}, false)).status, 403);
  assert.equal((await post('/api/sources', { codex_homes: ['relative'], expected_homes: initial })).status, 400);
  const originalExport = await exportJob();
  assert.equal(originalExport.value.snapshot.summary.usage.total_tokens, 12);

  const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
  browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
  const page = await browser.newPage({ viewport: { width: 1536, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' });
  await page.route(/\/api\/v2\/(projects|sessions)\?/, async route => {
    const response = await route.fetch();
    if (response.status() !== 200) return route.fulfill({ response });
    const listing = await response.json();
    if (listing.current_device) listing.current_device.platform = 'Windows';
    return route.fulfill({ response, json: listing });
  });
  let selected = null, holdPicker = false, releasePicker;
  await page.route('**/api/sources/choose', async route => {
    assert.ok(route.request().headers()['x-codex-write-token']);
    if (holdPicker) await new Promise(resolve => { releasePicker = resolve; });
    await route.fulfill({ json: { path: selected } });
  });
  page.setDefaultTimeout(10000);
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await page.goto(base + '/?page=settings&period=all');
  const card = page.locator('section').filter({ has: page.getByRole('heading', { name: 'Codex 日志来源' }) });
  const add = card.getByRole('button', { name: '添加文件夹' });
  await add.waitFor();
  await until(async () => await add.isEnabled());
  assert.equal(await card.getByRole('textbox', { name: 'Codex 文件夹路径' }).count(), 0);
  await add.click();
  await until(async () => await add.isEnabled());
  assert.equal((await json('/api/sources')).length, 1, 'cancelling the picker must not change sources');
  selected = second;
  holdPicker = true;
  await add.focus();
  assert.equal(await add.evaluate(element => element === document.activeElement), true);
  await add.hover();
  await card.screenshot({ path: path.join(output, 'hover-focus.png') });
  await page.mouse.down();
  await card.screenshot({ path: path.join(output, 'pressed.png') });
  await page.mouse.up();
  await until(() => !!releasePicker);
  assert.equal(await add.isDisabled(), true);
  await card.screenshot({ path: path.join(output, 'busy.png') });
  holdPicker = false;
  releasePicker();
  await until(async () => (await json('/api/sources')).length === 2);
  await page.waitForFunction(() => document.querySelectorAll('.source-row').length === 2);
  let listing = await json('/api/v2/sessions?period=all');
  assert.equal(listing.page.summary.usage.total_tokens, 36);
  assert.equal(listing.current_device.sources.length, 2);
  const record = JSON.parse(await readFile(path.join(config, 'runtime', 'instance.json'), 'utf8'));
  assert.deepEqual(record.codex_homes, (await json('/api/health')).codex_homes);
  const combinedExport = await exportJob();
  assert.notEqual(combinedExport.id, originalExport.id);
  assert.equal(combinedExport.value.snapshot.summary.usage.total_tokens, 36);
  assert.equal(combinedExport.value.snapshot.codex_sources.length, 2);
  const oldBytes = Buffer.from(await (await fetch(base + '/api/export.json.gz?job=' + originalExport.id)).arrayBuffer());
  assert.equal(JSON.parse(gunzipSync(oldBytes).toString('utf8')).snapshot.codex_sources.length, 1);

  selected = first;
  await add.click();
  await card.getByRole('alert').getByText('此文件夹已添加').waitFor();
  const stale = await post('/api/sources', { codex_homes: [second], expected_homes: initial });
  assert.equal(stale.status, 409);
  assert.equal((await json('/api/sources')).length, 2);
  await card.locator('.source-row').first().getByRole('button').click();
  await until(async () => (await json('/api/sources')).length === 1);
  assert.equal((await json('/api/v2/sessions?period=all')).page.summary.usage.total_tokens, 24);
  await card.getByRole('button', { name: '恢复自动发现' }).click();
  await until(async () => (await json('/api/settings')).codex_homes.length === 0);
  await until(async () => (await json('/api/v2/sessions?period=all')).page.summary.usage.total_tokens === 12);
  await until(async () => await add.isEnabled());
  assert.equal(await card.locator('.source-row').getByRole('button').isDisabled(), true);

  selected = second;
  await add.click();
  await until(async () => (await json('/api/sources')).length === 2);
  for (const width of [390, 320]) {
    await page.setViewportSize({ width, height: 740 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    await card.screenshot({ path: path.join(output, 'mobile-' + width + '.png') });
  }
  assert.deepEqual(errors, []);
  await browser.close();
  browser = null;
  await stop();
  await start();
  listing = await json('/api/v2/sessions?period=all');
  assert.equal(listing.page.summary.usage.total_tokens, 36);
  assert.equal(listing.codex_sources.length, 2);
  const remote = structuredClone(originalExport.value);
  remote.device.short_code = 'remote-fixture';
  remote.device.label = 'Remote fixture';
  assert.equal((await post('/api/remotes/import', { snapshot: remote, label: 'Remote fixture' })).status, 200);
  assert.equal((await json('/api/v2/sessions?period=all')).page.summary.usage.total_tokens, 48);
  const current = (await json('/api/sources')).map(source => source.path);
  assert.equal((await post('/api/sources', { codex_homes: [first], expected_homes: current })).status, 200);
  listing = await json('/api/v2/sessions?period=all');
  assert.equal(listing.page.summary.usage.total_tokens, 24);
  assert.equal(listing.codex_sources.filter(source => source.is_remote).length, 1);
  assert.equal((await exportJob()).value.snapshot.summary.usage.total_tokens, 12);
  console.log('Web log sources: add, duplicate, removal, auto discovery, authorization, stale edits, exports, remote data, managed identity, restart and narrow layouts passed.');
} finally {
  if (browser) await browser.close();
  try { await stop(); } catch { if (child && child.exitCode === null) child.kill(); }
}
