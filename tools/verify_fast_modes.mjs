import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture, sessions } from './fixtures/clear_mint.mjs';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined });
const output = 'output/verification/fast-modes';
await mkdir(output, { recursive: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 1024 }, locale: 'zh-CN', reducedMotion: 'reduce' });
Object.assign(sessions[0], { service_tier: 'priority' });
Object.assign(sessions[1], { service_tier: 'fast' });
Object.assign(sessions[2], { service_tier: 'ultrafast' });
Object.assign(sessions[3], { service_tier: 'standard', service_tiers: ['priority', 'standard'] });
await installMintFixture(context);
await context.route('**/api/v2/events?*', async route => {
  const params = new URL(route.request().url()).searchParams;
  // Event fixtures preserve a real switch from Fast back to Standard.
  const events = ['priority', 'standard', 'ultrafast'].map((service_tier, i) => ({
    timestamp: `2026-10-22T01:0${i}:00Z`, model: 'GPT-6.1 Sol', service_tier,
    total_token_usage: { input_tokens: 100, output_tokens: 20, total_tokens: 120 },
  }));
  await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({
    id: params.get('id'), revision: params.get('snapshot_token'),
    events, offset: 0, total: 3, limit: 100, next_cursor: null, previous_cursor: null,
  }) });
});
const page = await context.newPage();
page.setDefaultTimeout(8000);
const errors = [];
page.on('pageerror', error => errors.push(String(error)));
const modes = locator => locator.locator('.session-metadata [data-service-tier]').evaluateAll(nodes => nodes.map(node => node.dataset.serviceTier));
try {
  await page.goto('http://127.0.0.1:8765/svelte?period=week');
  const group = page.locator('.task-group').filter({ has: page.locator('.task-row .title-text', { hasText: '重构登录流程' }) });
  await group.locator('.task-row').waitFor();
  assert.deepEqual(await modes(group.locator('.task-row')), ['fast', 'ultrafast']);
  assert.equal(await group.locator('.task-row [data-icon="lightning"]').count(), 2);
  if (await group.locator('.task-row').getAttribute('aria-expanded') !== 'true') await group.locator('.task-row').click();
  await group.locator('[data-uid="main"]').waitFor();
  assert.deepEqual(await modes(group.locator('[data-uid="main"]')), ['fast']);
  assert.deepEqual(await modes(group.locator('[data-uid="agent-a"]')), ['fast']);
  assert.deepEqual(await modes(group.locator('[data-uid="agent-b"]')), ['ultrafast']);
  await page.screenshot({ path: `${output}/projects.png` });

  await page.getByRole('button', { name: '按会话', exact: true }).click();
  const main = page.locator('.sessions-table [data-uid="main"]');
  await main.waitFor();
  assert.deepEqual(await modes(main), ['fast']);
  assert.deepEqual(await modes(page.locator('.sessions-table [data-uid="layout"]')), []);
  await main.click();
  const detail = page.locator('.detail-panel');
  await page.waitForFunction(() => document.querySelector('.detail-panel')?.getAttribute('aria-busy') === 'false');
  assert.deepEqual(await modes(detail), ['fast']);
  assert.equal(await detail.locator('.session-metadata [data-icon="lightning"]').count(), 1);
  await detail.locator('.event-scroll tbody tr').first().waitFor({ state: 'attached' });
  assert.deepEqual(await detail.locator('.event-scroll [data-service-tier]').evaluateAll(nodes => nodes.map(node => node.textContent.trim())), ['Fast', 'standard', 'Ultrafast']);
  assert.equal(await detail.locator('.event-scroll [data-icon="lightning"]').count(), 2);
  await page.screenshot({ path: `${output}/detail.png` });
  await detail.getByRole('button', { name: '关闭', exact: true }).click();
  for (const width of [390, 320]) {
    await page.setViewportSize({ width, height: 844 });
    assert.equal(await main.locator('[data-icon="lightning"]').isVisible(), true);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  }
  await page.screenshot({ path: `${output}/mobile.png` });
  assert.deepEqual(errors, []);
  const health = await (await context.request.get('http://127.0.0.1:8765/api/health')).json();
  await writeFile(`${output}/checks.json`, JSON.stringify({ passed: true, build_id: health.build_id, checks: ['grouped modes and subagents', 'Fast and priority aliases', 'Standard after historical Fast', 'detail and per-event tiers', 'lightning icons', '390px and 320px layouts'], errors }, null, 2));
  console.log('Fast modes: all browser checks passed');
} finally {
  await browser.close();
}
