import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { gzipSync } from 'node:zlib';
import { installMintFixture, sessions, summary, usage, stamp, localDeviceInfo } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 960 }, locale: 'zh-CN', timezoneId: 'Asia/Shanghai', reducedMotion: 'reduce' });
await installMintFixture(context);
await context.route('**/api/security/token', route => route.fulfill({ json: { token: 'settings-fixture-token' } }));
let remotes = [];
await context.route('**/api/v2/projects?**', route => route.fulfill({ contentType: 'application/json', body: JSON.stringify({
  page: { items: [], summary: summary(sessions), total: 0, offset: 0, limit: 100, revision: 'settings-test', query_token: 'settings-test' },
  facets: summary(sessions), metric_trend: { complete: true, points: [] },
  period: { key: 'week', start_date: '2026-10-19', end_date: '2026-10-25' }, generated_at: stamp,
  codex_sources: [], remotes, current_device_short_code: 'this-device',
  current_device: localDeviceInfo,
}) }));
await context.route('**/api/remotes/**', route => {
  const request = route.request(), url = new URL(request.url());
  assert.equal(request.headers()['x-codex-write-token'], 'settings-fixture-token');
  const json = value => route.fulfill({ contentType: 'application/json', body: JSON.stringify(value) });
  if (url.pathname.endsWith('import-file')) {
    assert.ok(request.postDataBuffer().length > 0);
    if (!url.searchParams.get('label')) return json({ needs_label: true, suggested_label: 'Test laptop' });
    remotes = [{ label: url.searchParams.get('label'), device_short_code: 'test-device', session_count: 5, usage: usage(), exported_at: stamp }];
  } else if (url.pathname.endsWith('rename')) remotes[0].label = request.postDataJSON().label;
  else if (url.pathname.endsWith('delete')) {
    assert.equal(request.postDataJSON().confirm, true);
    remotes = [];
  } else throw new Error('Unexpected remote request');
  return json({ ok: true });
});
await context.route('**/api/export/start', route => route.fulfill({ contentType: 'application/json', body: JSON.stringify({ id: 'settings-export', phase: 'ready', percent: 95 }) }));
await context.route('**/api/export.json.gz?**', route => route.fulfill({ contentType: 'application/gzip', headers: { 'content-disposition': 'attachment; filename="settings-test.json.gz"' }, body: gzipSync('{}') }));
const page = await context.newPage();
page.setDefaultTimeout(8000);
const errors = [], checks = [];
page.on('pageerror', error => errors.push(String(error)));
const output = 'output/verification/settings-page';
const base = process.env.DASHBOARD_URL || 'http://127.0.0.1:8765';
await mkdir(output, { recursive: true });
try {
  await page.goto(base + '/?period=week&session=main&view=sessions&timezone=Asia%2FShanghai');
  await page.locator('.sessions-table').waitFor();
  await page.locator('.detail-scroll').waitFor();
  await page.locator('.detail-panel').evaluate(el => window.__retainedDetail = el);
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await page.getByRole('heading', { name: '设置', exact: true }).waitFor();
  assert.equal(await page.locator('dialog:visible, .preferences:visible, .metrics:visible, .detail-panel:visible').count(), 0);
  await page.keyboard.press('Escape');
  assert.equal(new URL(page.url()).searchParams.get('session'), 'main', 'A retained hidden detail must not handle settings keyboard events');
  await page.getByRole('button', { name: '概览', exact: true }).click();
  assert.equal(await page.locator('.detail-panel').evaluate(el => el === window.__retainedDetail), true);
  await page.getByRole('button', { name: '设置', exact: true }).click();
  assert.equal(await page.locator('.main-nav [aria-current="page"]').getAttribute('aria-label'), '设置');
  assert.equal(new URL(page.url()).searchParams.get('page'), 'settings');
  const localDevice = page.locator('.local-device-row');
  await localDevice.getByText('Local workstation', { exact: true }).waitFor();
  assert.match(await localDevice.innerText(), /this-device · 123 会话数/);
  const localSources = localDevice.locator('.local-sources li');
  assert.deepEqual(await localSources.allTextContents(), ['Windows23 会话数', 'WSL100 会话数']);
  assert.equal(await localDevice.getByText('本机', { exact: true }).count(), 1);
  assert.equal(await localDevice.getByRole('button').count(), 2);
  assert.equal(await localDevice.getByRole('button', { name: '完整导出', exact: true }).count(), 1);
  assert.equal(await localDevice.getByRole('button', { name: '脱敏导出', exact: true }).count(), 1);
  assert.equal(await page.locator('.remote-actions').getByRole('button').count(), 1);
  await page.reload();
  await page.getByRole('heading', { name: '设置', exact: true }).waitFor();
  await page.getByRole('button', { name: '概览', exact: true }).click();
  await page.locator('.sessions-table').waitFor();
  assert.equal(new URL(page.url()).searchParams.get('session'), 'main');
  assert.equal(new URL(page.url()).searchParams.get('view'), 'sessions');
  await page.goBack();
  await page.getByRole('heading', { name: '设置', exact: true }).waitFor();
  await page.goForward();
  await page.locator('.sessions-table').waitFor();
  await page.getByRole('button', { name: '设置', exact: true }).click();
  checks.push('Settings is a refreshable page; navigation and browser history preserve overview grouping and selection');

  const zone = page.locator('.dropdown > summary[aria-label="显示时区"]');
  await zone.click();
  const zones = page.getByRole('listbox', { name: '显示时区', exact: true });
  await zones.waitFor();
  assert.ok(await zones.getByRole('option').count() > 400);
  assert.equal(await zones.getByRole('option', { name: '上海 (UTC+08:00)', exact: true }).getAttribute('aria-selected'), 'true');
  await page.getByRole('textbox', { name: '搜索时区', exact: true }).fill('上海');
  assert.equal(await zones.getByRole('option').count(), 1);
  await page.getByRole('textbox', { name: '搜索时区', exact: true }).fill('no-such-zone');
  await page.getByRole('status').getByText('没有匹配的时区', { exact: true }).waitFor();
  await page.getByRole('textbox', { name: '搜索时区', exact: true }).fill('Shanghai');
  assert.equal(await zones.getByRole('option').count(), 1);
  await page.getByRole('textbox', { name: '搜索时区', exact: true }).fill('协调');
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  assert.equal(new URL(page.url()).searchParams.get('timezone'), 'UTC');
  assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('cousash:v2:timezone'))), 'UTC');
  await page.goto(base + '/?page=settings');
  await zone.waitFor();
  assert.equal(await zone.innerText(), '协调世界时 (UTC+00:00)');
  await zone.click();
  await page.keyboard.press('Escape');
  assert.equal(await zone.getAttribute('aria-expanded'), 'false');
  assert.equal(await zone.evaluate(el => el === document.activeElement), true);
  await page.getByLabel('语言', { exact: true }).first().click();
  await page.getByRole('option', { name: 'English', exact: true }).click();
  await page.getByRole('heading', { name: 'Settings', exact: true }).waitFor();
  const englishZone = page.locator('.dropdown > summary[aria-label="Display timezone"]');
  assert.equal(await englishZone.innerText(), 'Coordinated Universal Time (UTC+00:00)');
  await englishZone.click();
  await page.getByRole('textbox', { name: 'Search time zones', exact: true }).fill('Shanghai');
  await page.getByRole('option', { name: 'Shanghai (UTC+08:00)', exact: true }).waitFor();
  await page.keyboard.press('Escape');
  assert.equal(new URL(page.url()).searchParams.get('timezone'), 'UTC');
  assert.equal(await localDevice.getByText('This device', { exact: true }).count(), 1);
  assert.deepEqual(await localSources.allTextContents(), ['Windows23 Sessions', 'WSL100 Sessions']);
  assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('cousash:v2:language'))), 'en');
  await page.getByLabel('Language', { exact: true }).first().click();
  await page.getByRole('option', { name: '中文', exact: true }).click();
  checks.push('Language and time zone apply immediately; all supported time zones are selectable with keyboard support, Escape and persistent selection');

  await page.locator('input[type=file]').setInputFiles({ name: 'snapshot.json.gz', mimeType: 'application/gzip', buffer: gzipSync('{}') });
  await page.getByLabel('显示名称', { exact: true }).fill('Test laptop');
  await page.getByRole('button', { name: '确认', exact: true }).click();
  await page.locator('.remote-row').getByText('Test laptop', { exact: true }).waitFor();
  const importedDevice = page.locator('.imported-device-row');
  assert.equal(await importedDevice.getByText('外部导入', { exact: true }).count(), 1);
  assert.equal(await importedDevice.getByRole('button', { name: /完整导出|脱敏导出/ }).count(), 0);
  assert.match(await localDevice.innerText(), /this-device · 123 会话数/);
  assert.deepEqual(await localSources.allTextContents(), ['Windows23 会话数', 'WSL100 会话数']);
  checks.push('Windows and WSL have separate local counts that persist across import and language changes');
  const exportBounds = await localDevice.getByRole('button', { name: '完整导出', exact: true }).boundingBox();
  const nameBounds = await localDevice.locator('.device-name').boundingBox();
  assert.ok(exportBounds.x >= nameBounds.x + nameBounds.width, 'Export is to the right of local device data');
  checks.push('Local identity and unfiltered session count stay distinct from imports; only the local row exports');
  await page.locator('.remote-row').getByRole('button', { name: '改名', exact: true }).click();
  await page.getByLabel('显示名称', { exact: true }).fill('Renamed laptop');
  await page.locator('.remote-row').getByRole('button', { name: '确认', exact: true }).click();
  await page.locator('.remote-row').getByText('Renamed laptop', { exact: true }).waitFor();
  await page.screenshot({ path: `${output}/desktop.png` });
  const downloadEvent = page.waitForEvent('download');
  await localDevice.getByRole('button', { name: '完整导出', exact: true }).click();
  assert.equal((await downloadEvent).suggestedFilename(), 'settings-test.json.gz');
  await page.locator('.remote-row').getByRole('button', { name: '删除', exact: true }).click();
  await page.locator('.remote-row').getByRole('button', { name: '取消', exact: true }).click();
  assert.equal(remotes.length, 1);
  await page.locator('.remote-row').getByRole('button', { name: '删除', exact: true }).click();
  await page.locator('.remote-row').getByRole('button', { name: '确认', exact: true }).click();
  await page.getByText('尚未导入远端快照', { exact: true }).waitFor();
  assert.equal(await page.locator('dialog').count(), 0);
  checks.push('Compressed snapshot import, inline naming, rename, export download, cancel and confirmed deletion');

  for (const width of [390, 320]) {
    await page.setViewportSize({ width, height: 740 });
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    assert.equal(await localSources.count(), 2);
    await zone.click();
    await zones.waitFor();
    const zoneBounds = await zones.boundingBox();
    assert.ok(zoneBounds.x >= 0 && zoneBounds.x + zoneBounds.width <= width);
    assert.ok(zoneBounds.y >= 0 && zoneBounds.y + zoneBounds.height <= 740);
    await page.screenshot({ path: `${output}/mobile-${width}-timezone.png` });
    const nextZone = width === 390 ? 'Asia/Shanghai' : 'UTC';
    await page.getByRole('textbox', { name: '搜索时区', exact: true }).fill(nextZone);
    await zones.locator(`[data-option="${nextZone}"]`).click();
    assert.equal(new URL(page.url()).searchParams.get('timezone'), nextZone);
    await page.screenshot({ path: `${output}/mobile-${width}.png` });
    await page.getByRole('button', { name: '概览', exact: true }).click();
    await page.getByRole('button', { name: '设置', exact: true }).click();
    await page.getByRole('heading', { name: '设置', exact: true }).waitFor();
  }
  checks.push('320px and 390px layouts have no horizontal overflow; settings remain reachable');
  assert.deepEqual(errors, []);
  await writeFile(`${output}/report.json`, JSON.stringify({ checks, errors }, null, 2));
  console.log(checks.join('\n'));
} finally { await browser.close(); }
