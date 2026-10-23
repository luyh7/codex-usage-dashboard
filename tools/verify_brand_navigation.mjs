import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const output = 'output/verification/brand-navigation';
await mkdir(output, { recursive: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 1024 }, locale: 'zh-CN', timezoneId: 'Asia/Shanghai' });
await installMintFixture(context);
await context.addInitScript(() => {
  window.__streams = { opened: 0, closed: 0 };
  window.EventSource = class extends EventTarget {
    constructor() { super(); window.__streams.opened++; }
    close() { window.__streams.closed++; }
  };
});
const page = await context.newPage();
const report = { checks: [], errors: [], documents: 0, listingRequests: 0, port: 8765 };
page.on('pageerror', error => report.errors.push(String(error)));
page.on('request', request => {
  if (request.isNavigationRequest() && request.frame() === page.mainFrame()) report.documents++;
  if (/\/api\/v2\/(projects|sessions)\?/.test(request.url())) report.listingRequests++;
});
const check = label => { report.checks.push(label); console.log(label); };
try {
  await page.goto('http://127.0.0.1:8765/?period=week&timezone=Asia%2FShanghai&sort=updated');
  await page.locator('.project-heading').first().waitFor();
  assert.equal(new URL(page.url()).pathname, '/');
  assert.equal(await page.locator('a[href="/legacy"], a[href="/svelte"]').count(), 0);
  await page.evaluate(() => { window.__originalApp = document.querySelector('.app-shell'); });
  const initialRequests = report.listingRequests;
  const initialUrl = page.url();
  assert.equal(await page.locator('.brand a, .brand button').count(), 0);
  assert.equal(await page.locator('.brand').getAttribute('href'), null);
  assert.equal(await page.locator('.brand').getAttribute('data-tooltip'), null);
  assert.equal(await page.locator('.brand').evaluate(brand => getComputedStyle(brand).cursor === 'pointer'), false);
  // A same-event-loop burst catches any accidental navigation or refresh handler.
  await page.locator('.brand').evaluate(brand => { for (let i = 0; i < 100; i++) brand.click(); });
  await page.locator('.brand').click();
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  assert.equal(report.documents, 1, 'Brand clicks must not reload the document');
  assert.equal(report.listingRequests, initialRequests, 'Brand clicks must not refetch the listing');
  assert.equal(page.url(), initialUrl, 'Brand clicks retain filters');
  assert.equal(await page.evaluate(() => window.__originalApp === document.querySelector('.app-shell')), true);
  assert.deepEqual(await page.evaluate(() => window.__streams), { opened: 1, closed: 0 });
  check('Brand has no navigation or hover affordance; 101 clicks retain the document, data and single subscription');

  await page.locator('.view-switch button').nth(1).click();
  await page.locator('.sessions-table').waitFor();
  await page.locator('.sessions-table .session-title').first().click();
  await page.locator('.detail-panel').waitFor();
  await page.locator('.calendar-trigger').click();
  await page.locator('.calendar-panel').waitFor();
  // The calendar is modal; dismiss it before using the sidebar.
  await page.keyboard.press('Escape');
  await page.locator('.calendar-panel').waitFor({ state: 'detached' });
  const beforeReturn = report.listingRequests;
  const detailUrl = page.url();
  await page.locator('.brand').click();
  assert.equal(page.url(), detailUrl, 'Brand is inert even while viewing a session');
  assert.equal(await page.locator('.detail-panel').count(), 1);
  assert.equal(report.listingRequests, beforeReturn);
  check('Brand does not return to overview or close the current session');

  const overview = page.getByRole('button', { name: '概览', exact: true });
  await overview.click();
  await page.locator('.project-heading').first().waitFor();
  await overview.press('Enter');
  assert.equal(report.documents, 1);
  assert.equal(report.listingRequests, beforeReturn + 1, 'Returning from sessions loads projects once');
  assert.equal(new URL(page.url()).searchParams.get('period'), 'week');
  assert.equal(new URL(page.url()).searchParams.has('session'), false);
  assert.equal(new URL(page.url()).searchParams.has('view'), false);
  assert.equal(await page.locator('.calendar-panel').count(), 0);
  assert.equal(await page.locator('.detail-panel').count(), 0);
  check('Overview alone returns from session details to projects; keyboard activation stays idempotent');
  assert.deepEqual(report.errors, []);
  await page.screenshot({ path: `${output}/overview.png` });
} finally {
  await writeFile(`${output}/report.json`, JSON.stringify(report, null, 2));
  await browser.close();
}
