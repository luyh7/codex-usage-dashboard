import assert from 'node:assert/strict';
import { mkdir, readFile } from 'node:fs/promises';
import { installMintFixture } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, locale: 'zh-CN', reducedMotion: 'reduce' });
await installMintFixture(context);
await context.route('**/api/settings', route => route.fulfill({ json: { schema_version: 1, language: 'zh', timezone: 'Asia/Shanghai', codex_homes: [] } }));
await context.route('**/api/sources', route => route.fulfill({ json: [{ path: '/demo/.codex', status: 'available' }] }));

const notes = await readFile('docs/releases/v1.1.3.md', 'utf8');
let response = { state: 'available', version: '1.1.3', notes };
let finishInstall, failInstall, installCalls = 0;
await context.exposeBinding('testNativeUpdate', async (_context, command, args) => {
  if (command === 'desktop_settings') return { hide_on_close: false, can_hide: true, autostart: false, version: '1.1.2', channel: 'test' };
  if (command !== 'check_update') throw new Error('Unexpected desktop command: ' + command);
  if (!args.install) return response;
  installCalls++;
  return new Promise((resolve, reject) => { finishInstall = resolve; failInstall = reject; });
});
await context.addInitScript(() => {
  window.__TAURI__ = { core: { invoke: (command, args) => window.testNativeUpdate(command, args) } };
});

const page = await context.newPage();
page.setDefaultTimeout(8000);
const errors = [];
page.on('pageerror', error => errors.push(String(error)));
const base = process.env.DASHBOARD_URL || 'http://127.0.0.1:8765';
const output = 'output/verification/update-dialog';
await mkdir(output, { recursive: true });
const check = page.getByRole('button', { name: '检查更新', exact: true });
const dialog = page.getByRole('dialog', { name: '发现新版本' });
const until = async predicate => {
  const deadline = Date.now() + 8000;
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error('Timed out waiting for the desktop bridge');
    await new Promise(resolve => setTimeout(resolve, 20));
  }
};

try {
  await page.goto(base + '/?page=settings');
  await check.waitFor();
  await check.click();
  await dialog.waitFor();
  assert.equal(await dialog.locator('.note-type').count(), 3);
  assert.equal(await dialog.locator('.note-type').first().innerText(), '功能');
  assert.match(await dialog.innerText(), /v1\.1\.3/);
  assert.match(await dialog.innerText(), /当前 v1\.1\.2/);
  assert.equal(await dialog.getByRole('button', { name: '更新', exact: true }).count(), 1);
  assert.equal(await page.locator('section[aria-labelledby="desktop-title"] > .notes').count(), 0);
  await page.screenshot({ path: output + '/desktop.png' });

  await page.keyboard.press('Escape');
  assert.equal(await dialog.count(), 0);
  assert.equal(await check.evaluate(el => el === document.activeElement), true);
  await check.click();
  await dialog.waitFor();
  await page.mouse.click(10, 10);
  assert.equal(await dialog.count(), 0, 'backdrop click dismisses the update');
  assert.equal(await check.evaluate(el => el === document.activeElement), true);

  response = { state: 'current' };
  await check.click();
  await page.getByRole('status').getByText('已是最新版本').waitFor();
  assert.equal(await dialog.count(), 0);
  response = { state: 'unavailable', message: 'unavailable' };
  await check.click();
  await page.getByRole('status').getByText('尚未配置自动更新，请使用新版完整安装包升级。').waitFor();
  response = { state: 'package', message: 'package' };
  await check.click();
  await page.getByRole('status').getByText('请通过包管理器或新版安装包更新。').waitFor();

  response = { state: 'available', version: '1.1.3', notes };
  await check.click();
  await dialog.waitFor();
  const update = dialog.getByRole('button', { name: '更新', exact: true });
  await update.hover();
  await dialog.screenshot({ path: output + '/hover.png' });
  await dialog.getByRole('button', { name: '关闭' }).focus();
  await page.keyboard.press('Tab');
  assert.equal(await update.evaluate(el => el === document.activeElement), true);
  assert.equal(await update.evaluate(el => el.matches(':focus-visible')), true);
  await dialog.screenshot({ path: output + '/focus.png' });
  await page.mouse.down();
  await dialog.screenshot({ path: output + '/pressed.png' });
  await page.mouse.up();
  await until(() => installCalls === 1 && !!failInstall);
  assert.equal(await update.isDisabled(), true);
  assert.equal(await dialog.getByRole('button', { name: '关闭' }).isDisabled(), true);
  await page.keyboard.press('Escape');
  assert.equal(await dialog.isVisible(), true, 'an in-flight update cannot be dismissed');
  await page.evaluate(() => window.dispatchEvent(new CustomEvent('cousash-update-progress', { detail: { received: 25, total: 100 } })));
  await dialog.getByText('25%', { exact: true }).waitFor();
  assert.equal(await dialog.getByRole('progressbar').getAttribute('value'), '25');
  await dialog.screenshot({ path: output + '/disabled-progress.png' });
  failInstall(new Error('Synthetic install failure'));
  await dialog.getByRole('alert').getByText(/Synthetic install failure/).waitFor();
  assert.equal(await update.isEnabled(), true);
  await update.click();
  await until(() => installCalls === 2 && !!finishInstall);
  assert.equal(await dialog.getByRole('alert').count(), 0);
  finishInstall({ state: 'current' });
  await page.getByRole('status').getByText('已是最新版本').waitFor();
  assert.equal(await dialog.count(), 0);

  await page.setViewportSize({ width: 320, height: 640 });
  response = { state: 'available', version: '1.1.3', notes: '## 功能 · Features\n' + Array.from({ length: 16 }, (_, i) => `- 新增第 ${i + 1} 项较长的说明内容。<br>Add a longer item ${i + 1}.`).join('\n') };
  await check.click();
  await dialog.waitFor();
  const dimensions = await dialog.evaluate(el => ({ rect: el.getBoundingClientRect().toJSON(), scroll: el.querySelector('.update-body').scrollHeight, client: el.querySelector('.update-body').clientHeight }));
  assert.ok(dimensions.rect.width <= 320 && dimensions.rect.height <= 640);
  assert.ok(dimensions.scroll > dimensions.client, 'long release notes scroll inside the dialog');
  assert.equal(await dialog.getByRole('button', { name: '更新', exact: true }).isVisible(), true);
  await page.screenshot({ path: output + '/narrow.png' });
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ available: true, current: true, unavailable: true, package: true, dismissal: true, progress: true, errorAndRetry: true, narrowAndLongNotes: true, screenshot: output + '/desktop.png' }));
} finally {
  await browser.close();
}
