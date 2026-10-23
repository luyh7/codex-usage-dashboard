import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture, session, summary, stamp } from './fixtures/clear_mint.mjs';
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const context = await browser.newContext({ viewport: { width: 1366, height: 900 }, reducedMotion: 'reduce' });
await installMintFixture(context);
const output = 'output/verification/multi-filters';
await mkdir(output, { recursive: true });
const rows = [['a', 'local', 'Model A', 100], ['b', 'wsl', 'Model B', 200], ['c', 'remote', 'Model B', 400], ['d', 'wsl', 'Model C', 800]].map(([id, environment, model, tokens]) => ({ ...session(id, id, tokens, tokens / 100), environment, environment_id: environment, model, models: [model] }));
const choices = value => !value ? [] : value.startsWith('[') ? JSON.parse(value) : [value];
let listingRequests = 0;
await context.route('**/api/v2/sessions?**', route => {
  listingRequests++;
  const p = new URL(route.request().url()).searchParams;
  const environments = choices(p.get('environment')), models = choices(p.get('model'));
  const items = rows.filter(row => (!environments.length || environments.includes(row.environment_id)) && (!models.length || models.includes(row.model)));
  return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({
    page: { items, total: items.length, offset: 0, limit: 100, revision: 'multi', query_token: 'multi', summary: summary(items) },
    facets: { ...summary(rows), by_model: ['Model A', 'Model B', 'Model C'].map(model => ({ model })) },
    period: { key: 'week', start_date: '2026-10-19', end_date: '2026-10-25' }, generated_at: stamp,
    codex_sources: ['local', 'wsl', 'remote'].map(id => ({ id, label: id })), remotes: [], current_device_short_code: 'this-device',
  }) });
});
const page = await context.newPage();
page.setDefaultTimeout(8000);
const errors = [], checks = [];
page.on('pageerror', error => errors.push(String(error)));
const panel = page.getByRole('dialog', { name: '筛选', exact: true });
const option = (group, name) => panel.getByRole('group', { name: group, exact: true }).getByRole('button', { name, exact: true });
const choose = (group, name) => option(group, name).click();
const expectRows = async count => {
  await page.waitForFunction(count => document.querySelectorAll('.sessions-table tbody tr').length === count, count);
};
const cost = async value => {
  await page.waitForFunction(value => document.querySelector('.cost-card strong')?.textContent.trim() === value, value);
};
const selected = async (group, names) => {
  const actual = await panel.getByRole('group', { name: group, exact: true }).locator('button[aria-pressed="true"]').allTextContents();
  assert.deepEqual(actual.sort(), [...names].sort());
};
const commit = () => panel.getByRole('button', { name: '确定', exact: true }).click();
const reopen = () => page.locator('.filter-toggle').click();
try {
  await page.goto('http://127.0.0.1:8765/?view=sessions&period=week');
  await expectRows(4); await page.waitForLoadState('networkidle');
  await reopen();
  assert.equal(await panel.getByText('全局筛选', { exact: true }).count(), 0);
  assert.equal(await panel.getByText('选择后即时生效', { exact: true }).count(), 0);
  const before = listingRequests, originalUrl = page.url();
  await choose('环境', 'local'); await choose('环境', 'wsl');
  await choose('模型', 'Model A'); await choose('模型', 'Model B');
  await selected('环境', ['local', 'wsl']); await selected('模型', ['Model A', 'Model B']);
  await choose('状态', '已归档'); await choose('状态', '活跃');
  await selected('状态', ['活跃']); await choose('状态', '全部');
  await choose('代理', '仅子代理'); await choose('代理', '主会话');
  await selected('代理', ['主会话']); await choose('代理', '全部');
  await expectRows(4); await cost('$15.00');
  assert.equal(page.url(), originalUrl);
  assert.equal(listingRequests, before, 'Draft choices must not query the server');
  assert.equal(await page.locator('.filter-chip').count(), 0);
  await page.screenshot({ path: `${output}/desktop-draft.png` });
  await selected('环境', ['local', 'wsl']); await selected('模型', ['Model A', 'Model B']);
  await commit();
  await expectRows(2); await cost('$3.00');
  assert.equal(await panel.isVisible(), false);
  assert.equal(listingRequests, before + 1, 'Confirm applies all four groups in one query');
  assert.deepEqual(choices(new URL(page.url()).searchParams.get('model')), ['Model A', 'Model B']);
  assert.equal(await page.locator('.filter-chip').count(), 4);
  checks.push('Header text is absent; options stay as drafts without requests or metric changes; Confirm applies the full selection in one query and closes the panel');
  await page.reload(); await expectRows(2);
  await reopen();
  await selected('环境', ['local', 'wsl']); await selected('模型', ['Model A', 'Model B']);
  const appliedUrl = page.url();
  for (const dismiss of ['escape', 'close', 'outside']) {
    await choose('模型', 'Model C');
    if (dismiss === 'escape') await page.keyboard.press('Escape');
    else if (dismiss === 'close') await panel.getByRole('button', { name: '关闭', exact: true }).click();
    else await page.locator('h1').click();
    assert.equal(page.url(), appliedUrl);
    await reopen(); await selected('模型', ['Model A', 'Model B']);
  }
  await choose('环境', 'remote');
  await panel.getByRole('button', { name: '清除筛选', exact: true }).click();
  await selected('环境', ['全部环境']); await selected('模型', ['全部模型']);
  assert.equal(page.url(), appliedUrl);
  await expectRows(2);
  await page.keyboard.press('Escape');
  await reopen(); await selected('环境', ['local', 'wsl']);
  await page.keyboard.press('Escape');
  checks.push('Applied multi-selection survives reload; Escape, close and outside clicks discard edits; Clear resets only the draft until confirmed');
  await page.getByRole('button', { name: '移除筛选: 环境: local', exact: true }).click();
  await expectRows(1); await cost('$2.00');
  await page.getByRole('button', { name: '移除筛选: 模型: Model A', exact: true }).click();
  assert.equal(new URL(page.url()).searchParams.get('model'), 'Model B');
  await reopen();
  await choose('环境', '全部环境');
  await expectRows(1);
  await commit(); await expectRows(2); await cost('$6.00');
  await reopen(); await choose('模型', '全部模型');
  await selected('模型', ['全部模型']);
  await commit(); await expectRows(4); await cost('$15.00');
  assert.equal(await page.locator('.filter-chip').count(), 0);
  await reopen();
  await option('环境', 'wsl').focus(); await page.keyboard.press('Enter');
  await selected('环境', ['wsl']); await expectRows(4);
  await page.keyboard.press('Space'); await selected('环境', ['全部环境']);
  await commit(); await expectRows(4);
  await page.setViewportSize({ width: 320, height: 844 });
  await reopen();
  await choose('环境', 'local'); await choose('环境', 'wsl');
  await choose('模型', 'Model A'); await choose('模型', 'Model B');
  await expectRows(4);
  const confirmBox = await panel.getByRole('button', { name: '确定', exact: true }).boundingBox();
  assert.ok(confirmBox && confirmBox.y >= 0 && confirmBox.y + confirmBox.height <= 844);
  await page.screenshot({ path: `${output}/mobile.png` });
  await commit(); await expectRows(2);
  await page.locator('.clear-filters').click(); await expectRows(4);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  checks.push('Chips still remove applied conditions immediately; group resets, keyboard toggles and the visible mobile Confirm button work');
  assert.deepEqual(errors, []);
  await writeFile(`${output}/checks.json`, JSON.stringify({ passed: true, checks, errors }, null, 2));
  console.log(checks.join('\n'));
} catch (error) {
  await page.screenshot({ path: `${output}/failure.png` }); throw error;
} finally { await browser.close(); }
