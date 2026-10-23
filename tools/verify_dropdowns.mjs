import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture } from './fixtures/clear_mint.mjs';
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const output = 'output/verification/dropdowns';
await mkdir(output, { recursive: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 1024 }, locale: 'zh-CN', reducedMotion: 'reduce' });
await installMintFixture(context);
const page = await context.newPage();
page.setDefaultTimeout(8000);
const errors = [], checks = [];
page.on('pageerror', error => errors.push(String(error)));
const trigger = label => page.locator(`.dropdown > summary[aria-label="${label}"]`);
const select = async (label, option) => {
  await trigger(label).click();
  await page.getByRole('option', { name: option, exact: true }).click();
};
const check = message => { checks.push(message); console.log(message); };
const bounds = async role => {
  const list = page.getByRole(role);
  await list.waitFor();
  const box = await list.boundingBox();
  const size = page.viewportSize();
  assert.ok(box.x >= 0 && box.y >= 0 && box.x + box.width <= size.width && box.y + box.height <= size.height, 'Dropdown stays inside viewport');
  assert.equal(await list.evaluate(el => el.matches(':popover-open')), true);
  return list;
};
try {
  await page.goto('http://127.0.0.1:8765/?period=week&timezone=Asia%2FShanghai');
  await page.locator('.task-row').first().waitFor();
  assert.equal(await page.locator('select').count(), 0, 'No native selects remain in the current UI');
  // Global filters now expose direct choices; their behavior is covered by verify_global_filters.mjs.
  await page.locator('.preferences > summary').click();
  await trigger('语言').click();
  await bounds('listbox');
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('.preferences').evaluate(el => el.open), true, 'First Escape closes only nested dropdown');
  await select('密度', '紧凑');
  await page.waitForFunction(() => document.documentElement.dataset.density === 'compact');
  assert.equal(await page.locator('.preferences').evaluate(el => el.open), true);
  await select('密度', '舒适');
  await select('语言', 'English');
  await page.waitForFunction(() => document.documentElement.lang === 'en');
  await select('Language', '中文');
  await page.waitForFunction(() => document.documentElement.lang === 'zh-CN');
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('.preferences').evaluate(el => el.open), false);
  check('Preferences use the same component; language/density persist and nested Escape keeps the parent panel open');

  await page.locator('.calendar-trigger').click();
  assert.equal(await page.locator('input[type="month"]').count(), 0);
  await trigger('月').click();
  await bounds('listbox');
  assert.equal(await page.getByRole('option').count(), 12);
  await page.getByRole('option', { name: '9月', exact: true }).click();
  assert.match(await page.locator('.calendar-grid').getAttribute('aria-label'), /9月/);
  await page.getByLabel('年', { exact: true }).fill('2025');
  await page.getByLabel('年', { exact: true }).press('Tab');
  assert.match(await page.locator('.calendar-grid').getAttribute('aria-label'), /2025/);
  await trigger('更多').click();
  await bounds('menu');
  await page.screenshot({ path: `${output}/calendar-more.png` });
  await page.keyboard.press('Escape');
  assert.equal(await page.locator('.calendar-panel').count(), 1, 'Escape closes the calendar menu before the modal');
  await trigger('更多').click();
  await page.getByRole('menuitemradio', { name: '近 30 天', exact: true }).click();
  assert.equal(await page.locator('.calendar-panel').count(), 1);
  assert.equal(new URL(page.url()).searchParams.get('period'), 'week', 'Calendar choice remains a draft');
  await page.keyboard.press('Escape');
  check('Calendar More has the same menu style, stays above the modal and preserves draft selection');

  for (const width of [390, 320]) {
    await page.setViewportSize({ width, height: 844 });
    await trigger('时间范围').click();
    await bounds('listbox');
    await page.getByRole('option', { name: '近 7 天', exact: true }).click();
    assert.equal(new URL(page.url()).searchParams.get('period'), '7d');
    await trigger('时间范围').click();
    await page.getByRole('option', { name: '自定义日期', exact: true }).click();
    await page.locator('.calendar-panel').waitFor();
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: /^筛选/ }).click();
    await page.getByRole('dialog', { name: '筛选' }).waitFor();
    await page.screenshot({ path: `${output}/mobile-${width}.png` });
    await page.keyboard.press('Escape');
    await page.locator('h1').click();
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  }
  check('Mobile date-range and filter dropdowns fit 390/320px screens; custom range still opens the calendar');
  assert.deepEqual(errors, []);
  await writeFile(`${output}/checks.json`, JSON.stringify({ passed: true, checks, errors }, null, 2));
} catch (error) {
  await page.screenshot({ path: `${output}/failure.png` });
  throw error;
} finally { await browser.close(); }
