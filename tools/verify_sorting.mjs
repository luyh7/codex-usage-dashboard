import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const output = 'output/verification/sort-icon';
await mkdir(output, { recursive: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 1024 }, locale: 'zh-CN', reducedMotion: 'reduce' });
await installMintFixture(context);
const page = await context.newPage();
page.setDefaultTimeout(8000);
const errors = [], checks = [];
page.on('pageerror', error => errors.push(String(error)));
const direction = page.locator('.sort-direction');
const trigger = page.locator('.sort-menu > summary');
const menu = page.locator('.sort-menu [role="menu"]');
const check = message => { checks.push(message); console.log(message); };
const waitDirection = ascending => page.waitForFunction(ascending => document.querySelector('.sort-direction')?.getAttribute('aria-pressed') === String(ascending), ascending);
try {
  await page.goto('http://127.0.0.1:8765/svelte?period=week&timezone=Asia%2FShanghai');
  await page.locator('.task-row').first().waitFor();
  await waitDirection(false);
  assert.equal(await direction.getAttribute('aria-label'), '降序');
  const descendingPath = await direction.locator('path').getAttribute('d');
  await page.locator('.sorting').screenshot({ path: `${output}/descending.png` });
  await direction.click();
  await waitDirection(true);
  assert.equal(await direction.getAttribute('aria-label'), '升序');
  const ascendingPath = await direction.locator('path').getAttribute('d');
  assert.notEqual(ascendingPath, descendingPath, 'Direction uses distinct SVG geometry');
  assert.equal(await direction.locator('[data-icon="sort-ascending"]').count(), 1);
  assert.equal(new URL(page.url()).searchParams.get('ascending'), 'true');
  assert.equal(await trigger.innerText(), '最近更新');
  assert.equal(await trigger.getAttribute('aria-expanded'), 'false', 'Direction button still reverses sorting directly');
  await page.mouse.move(50, 50);
  await page.locator('.sorting').screenshot({ path: `${output}/ascending.png` });
  check('Distinct ascending/descending arrows replace the symmetric icon; clicking still reverses sorting directly');

  await direction.focus();
  await page.keyboard.press('Space');
  await waitDirection(false);
  assert.equal(await direction.locator('[data-icon="sort-descending"]').count(), 1);
  await trigger.click();
  await menu.waitFor({ state: 'visible' });
  assert.equal(await menu.getByRole('menuitemradio').count(), 4, 'Original sort options are preserved');
  await menu.getByRole('menuitemradio', { name: '费用', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('.sort-menu > summary')?.textContent.trim() === '费用');
  await direction.click();
  await waitDirection(true);
  assert.equal(await trigger.innerText(), '费用');
  check('Keyboard reversal and original four-option sorting menu keep their existing behavior');

  await page.reload();
  await page.locator('.task-row').first().waitFor();
  await waitDirection(true);
  assert.equal(await direction.locator('[data-icon="sort-ascending"]').count(), 1);
  for (const width of [390, 320]) {
    await page.setViewportSize({ width, height: 844 });
    await direction.click();
    await waitDirection(false);
    await direction.click();
    await waitDirection(true);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    const box = await direction.boundingBox();
    assert.ok(box.x >= 0 && box.x + box.width <= width);
  }
  check('Correct icon survives reload and stays usable at 390/320px');
  assert.deepEqual(errors, []);
  await writeFile(`${output}/checks.json`, JSON.stringify({ passed: true, checks, errors }, null, 2));
} catch (error) {
  await page.screenshot({ path: `${output}/failure.png` });
  throw error;
} finally { await browser.close(); }
