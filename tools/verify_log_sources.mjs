import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 960 }, locale: 'zh-CN', reducedMotion: 'reduce' });
const original = '\\\\?\\C:\\Users\\Example\\.codex';
const wsl = '\\\\wsl.localhost\\Ubuntu\\home\\example\\.codex';
const other = 'D:\\codex-alt';
let homes = [], sources = [{ path: original, status: 'available' }], selected = null;
let requests = [], failApply = false, failSources = false, holdApply = false, releaseApply;
await context.exposeBinding('testNative', async (_context, command, args) => {
  if (command === 'desktop_settings') return { hide_on_close: false, can_hide: true, autostart: false, version: '1.1.0', channel: 'test' };
  if (command === 'choose_source') return selected;
  if (command !== 'apply_sources') throw new Error('Unexpected desktop command: ' + command);
  requests.push(args.codexHomes);
  if (holdApply) await new Promise(resolve => { releaseApply = resolve; });
  if (failApply) { failApply = false; throw new Error('Synthetic source failure'); }
  homes = args.codexHomes;
  sources = (homes.length ? homes : [original]).map(path => ({ path, status: 'available' }));
});
await context.addInitScript(() => {
  localStorage.setItem('cousash:v2:language', JSON.stringify('zh'));
  window.__TAURI__ = { core: { invoke: (command, args) => window.testNative(command, args) } };
});
await context.route('**/api/settings', async route => {
  if (route.request().method() === 'GET') return route.fulfill({ json: { schema_version: 1, language: 'zh', timezone: 'Asia/Shanghai', codex_homes: homes } });
  return route.fulfill({ json: { schema_version: 1, language: 'zh', timezone: 'Asia/Shanghai', codex_homes: homes } });
});
await context.route('**/api/sources', route => failSources
  ? route.fulfill({ status: 500, json: { error: 'Synthetic source read failure' } })
  : route.fulfill({ json: sources }));

const page = await context.newPage();
  await page.route(/\/api\/v2\/(projects|sessions)\?/, async route => {
    const response = await route.fetch();
    if (response.status() !== 200) return route.fulfill({ response });
    const listing = await response.json();
    if (listing.current_device) listing.current_device.platform = 'Windows';
    return route.fulfill({ response, json: listing });
  });
page.setDefaultTimeout(8000);
const errors = [];
page.on('pageerror', error => errors.push(String(error)));
const base = process.env.DASHBOARD_URL || 'http://127.0.0.1:8765';
const output = 'output/verification/log-sources';
await mkdir(output, { recursive: true });
const card = page.locator('section').filter({ has: page.getByRole('heading', { name: 'Codex 日志来源' }) });
const add = card.getByRole('button', { name: '添加文件夹' });
const remove = () => card.getByRole('button', { name: /^移除:/ });
const until = async test => {
  const deadline = Date.now() + 8000;
  while (!test()) {
    if (Date.now() > deadline) throw new Error('Timed out waiting for the desktop bridge');
    await new Promise(resolve => setTimeout(resolve, 20));
  }
};
const reopen = async () => {
  await page.reload();
  await add.waitFor({ state: 'visible' });
  await until(() => sources.length === homes.length || homes.length === 0);
  await card.locator('.source-row').last().waitFor();
};
try {
  await page.goto(base + '/?page=settings');
  await add.waitFor({ state: 'visible' });
  await page.waitForFunction(() => !document.querySelector('.source-actions button')?.disabled);
  assert.equal(await remove().count(), 1);
  assert.equal(await remove().first().isDisabled(), true);
  assert.equal(await card.locator('.source-path').innerText(), 'C:\\Users\\Example\\.codex');
  await card.screenshot({ path: output + '/default.png' });
  await add.hover();
  await card.screenshot({ path: output + '/hover.png' });
  await add.focus();
  assert.equal(await add.evaluate(el => el === document.activeElement), true);
  await card.screenshot({ path: output + '/focus.png' });
  await page.mouse.down();
  await card.screenshot({ path: output + '/pressed.png' });
  await page.mouse.up();
  await until(() => !holdApply && requests.length === 0);

  selected = 'c:/users/example/.codex/';
  await add.click();
  await card.getByRole('alert').getByText('此文件夹已添加').waitFor();
  assert.equal(requests.length, 0, 'duplicates must not restart the service');

  selected = wsl;
  holdApply = true;
  await add.click();
  await until(() => !!releaseApply);
  assert.deepEqual(requests.at(-1), [original, wsl], 'adding preserves discovered homes');
  assert.equal(await add.isDisabled(), true);
  assert.equal(await remove().first().isDisabled(), true);
  await card.screenshot({ path: output + '/busy.png' });
  holdApply = false;
  releaseApply();
  await reopen();
  assert.equal(await remove().count(), 2);
  assert.equal(await remove().first().isEnabled(), true);

  selected = other;
  failApply = true;
  await add.click();
  await card.getByRole('alert').getByText('Synthetic source failure', { exact: false }).waitFor();
  assert.equal(await remove().count(), 2, 'a failed change must keep existing sources');
  await add.click();
  await until(() => homes.length === 3);
  assert.deepEqual(homes, [original, wsl, other]);
  await reopen();
  assert.equal(await remove().count(), 3);
  await remove().nth(1).click();
  await until(() => homes.length === 2);
  assert.deepEqual(homes, [original, other]);
  await reopen();
  await card.getByRole('button', { name: '恢复自动发现' }).click();
  await until(() => homes.length === 0);
  await reopen();
  assert.equal(await remove().count(), 1);
  assert.equal(await remove().first().isDisabled(), true);

  selected = wsl;
  await add.click();
  await until(() => homes.length === 2);
  await reopen();
  for (const width of [390, 320]) {
    await page.setViewportSize({ width, height: 740 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    assert.equal(await remove().count(), 2);
    await card.screenshot({ path: output + '/mobile-' + width + '.png' });
  }
  failSources = true;
  await page.reload();
  await card.getByRole('alert').getByText('Cannot read log sources', { exact: false }).waitFor();
  assert.equal(await add.isDisabled(), true, 'cannot replace unknown discovered sources');
  assert.deepEqual(errors, []);
  console.log('Source settings: discovered homes preserved; duplicate, cancel, busy, failure, remove, reset, and narrow layouts passed.');
} finally {
  await browser.close();
}
