import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture, sessions, summary, usage, stamp } from './fixtures/clear_mint.mjs';
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 960 }, reducedMotion: 'no-preference' });
await installMintFixture(context);
let models = [
  { model: 'gpt-6.1-sol', usage: usage(8000000), estimated_cost_usd: 2, cost_complete: true },
  { model: 'gpt-6-astra', usage: usage(2000000), estimated_cost_usd: 6, cost_complete: true },
  { model: 'unknown', usage: usage(100000), estimated_cost_usd: null, cost_complete: false },
  ...['free-a','free-b','free-model-with-a-very-long-name'].map(model => ({model,usage:usage(0),estimated_cost_usd:0,cost_complete:true})),
];
await context.route('**/api/v2/projects?**', route => {
  const s = { ...summary(sessions), usage: usage(10100000), estimated_cost_usd: 8, estimated_cost_known_count: 9, model_breakdown: models };
  return route.fulfill({ contentType: 'application/json', body: JSON.stringify({
    page: { items: [], summary: s, total: 0, offset: 0, limit: 100, revision: 'models', query_token: 'models' },
    facets: summary(sessions), metric_trend: { complete: true, points: [] },
    period: { key: 'week', start_date: '2026-10-19', end_date: '2026-10-25' }, generated_at: stamp,
    codex_sources: [], remotes: [], current_device_short_code: 'this-device',
  }) });
});
const page = await context.newPage();
page.setDefaultTimeout(8000);
const errors = [], checks = [];
page.on('pageerror', e => errors.push(String(e)));
const output = 'output/verification/model-breakdown';
await mkdir(output, { recursive: true });
const cost = page.locator('.cost-toggle'), panel = page.locator('.model-panel'), border = page.locator('.model-border');
async function settle() {
  await page.waitForFunction(() => document.querySelector('.metrics')?.getAnimations({subtree:true}).every(a =>
    a instanceof CSSAnimation || a.playState === 'finished' || a.playState === 'idle'));
}
async function startSwapAudit() {
  await page.evaluate(() => {
    const audit = window.__modelSwapAudit = { running: true, frames: 0, overlaps: [] };
    function sample() {
      if (!audit.running) return;
      audit.frames++;
      const cost = document.querySelector('.cost-card').getBoundingClientRect();
      for (const node of document.querySelectorAll('.token-card,.sessions-card,.cache-card,.model-panel,.model-overflow')) {
        const rect = node.getBoundingClientRect();
        if (Number(getComputedStyle(node).opacity) > .05 &&
          Math.min(rect.right,cost.right)-Math.max(rect.left,cost.left)>1 &&
          Math.min(rect.bottom,cost.bottom)-Math.max(rect.top,cost.top)>1) {
          audit.overlaps.push(node.className);
        }
      }
      requestAnimationFrame(sample);
    }
    requestAnimationFrame(sample);
  });
}
async function finishSwapAudit() {
  const audit = await page.evaluate(() => {
    window.__modelSwapAudit.running = false;
    return window.__modelSwapAudit;
  });
  assert.ok(audit.frames > 0, 'The swap was sampled during rendering');
  assert.deepEqual(audit.overlaps, [], 'Visible cards never overlap the cost card during a swap');
}
async function bounds() {
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
  const box = await panel.boundingBox(), view = page.viewportSize();
  assert.ok(box.x >= 0 && box.x + box.width <= view.width && box.height > 80);
  const cards = await panel.locator('.model-card').evaluateAll(nodes => nodes.map(node => {
    const r = node.getBoundingClientRect(); return {x:r.x,y:r.y,right:r.right,bottom:r.bottom};
  }));
  const columns = view.width > 600 ? 3 : 2;
  assert.equal(cards.length, 6);
  assert.ok(Math.abs(cards[0].y - cards[columns - 1].y) < 1);
  assert.ok(cards[columns].y > cards[0].y);
  for (const card of cards) assert.ok(card.x >= box.x && card.right <= box.x + box.width + 1 && card.bottom <= box.y + box.height + 1);
  assert.equal(await panel.evaluate(el => getComputedStyle(el).borderTopStyle), 'none', 'Grid has no enclosing card frame');
  assert.equal(await panel.locator('table').count(), 0);
}
async function show() {
  await cost.click();
  await panel.waitFor();
  await settle();
  assert.equal(await page.locator('.token-card, .sessions-card, .cache-card').count(), 0);
}
async function hide() {
  await cost.click();
  await panel.waitFor({state:'detached'});
  await settle();
  assert.equal(await page.locator('.token-card, .sessions-card, .cache-card').count(), 3);
}
try {
  await page.goto('http://127.0.0.1:8765/svelte?period=week&timezone=Asia%2FShanghai');
  await page.waitForFunction(() => document.querySelector('.cost-toggle')?.disabled === false);
  await page.locator('h1').hover();
  assert.equal(await border.evaluate(el => getComputedStyle(el).opacity), '0');
  assert.equal(await page.locator('.cost-card').evaluate(el => getComputedStyle(el).borderTopColor), 'rgba(0, 0, 0, 0)');
  await cost.hover();
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.model-border')).opacity === '1');
  const segments = page.locator('.model-border .border-segment');
  assert.equal(await segments.count(), 2);
  const ratios = await segments.evaluateAll(rows => rows.map(el => el.getAttribute('stroke-dasharray')));
  assert.deepEqual(ratios, ['75 25', '25 75']);
  const moving = await segments.first().evaluate(el => getComputedStyle(el).strokeDashoffset);
  await page.waitForFunction(old => getComputedStyle(document.querySelector('.model-border .border-segment')).strokeDashoffset !== old, moving);
  const ink = border.locator('.border-ink');
  assert.equal(await border.locator('.border-glint, [stroke="white"]').count(), 0, 'No white sweep overlays the model colors');
  assert.equal(await border.evaluate(el => getComputedStyle(el).transform), 'none', 'Border geometry never scales');
  const borderSize = await border.boundingBox();
  const breathing = await ink.evaluate(el => getComputedStyle(el).strokeWidth);
  await page.waitForFunction(old => getComputedStyle(document.querySelector('.border-ink')).strokeWidth !== old, breathing);
  const changedSize = await border.boundingBox();
  assert.equal(changedSize.width, borderSize.width);
  assert.equal(changedSize.height, borderSize.height);
  assert.equal(await ink.evaluate(el => getComputedStyle(el).animationPlayState), 'running');
  await page.screenshot({path:`${output}/animated-border.png`});
  await page.locator('h1').hover();
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.model-border')).opacity === '0');
  checks.push('Hover reveals a continuously moving perimeter with exact 75/25 cost shares; leaving hides it');
  const before = await page.locator('.workspace').boundingBox();
  await startSwapAudit();
  await cost.click();
  await page.waitForFunction(() => document.querySelector('.metrics').getAnimations({subtree:true}).some(a => !(a instanceof CSSAnimation) && a.playState === 'running'));
  await panel.waitFor();
  await settle();
  await finishSwapAudit();
  assert.equal(await page.locator('.token-card, .sessions-card, .cache-card').count(), 0);
  assert.deepEqual(await panel.locator('.model-name').allTextContents(), ['gpt-6-astra','gpt-6.1-sol','free-a','free-b','free-model-with-a-very-long-name','未知']);
  assert.ok((await panel.innerText()).includes('8.00M'));
  const after = await page.locator('.workspace').boundingBox();
  assert.ok(Math.abs(before.y - after.y) < 1, 'Desktop list stays in place during the swap');
  await bounds();
  await panel.locator('.model-usage').first().hover();
  assert.equal(await page.getByRole('tooltip').count(), 0);
  assert.equal(await border.evaluate(el => getComputedStyle(el).opacity), '1');
  assert.equal(await ink.evaluate(el => getComputedStyle(el).animationPlayState), 'running');
  const expandedMoving = await segments.first().evaluate(el => getComputedStyle(el).strokeDashoffset);
  await page.waitForFunction(old => getComputedStyle(document.querySelector('.model-border .border-segment')).strokeDashoffset !== old, expandedMoving);
  await page.screenshot({path:`${output}/models-expanded.png`});
  await startSwapAudit();
  await hide();
  await finishSwapAudit();
  await page.locator('h1').hover();
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.model-border')).opacity === '0');
  assert.equal(await segments.first().evaluate(el => getComputedStyle(el).animationPlayState), 'paused');
  checks.push('Expanded border keeps flowing without hover; collapsed card has no idle border even when its button retains focus');
  await cost.focus();
  await page.keyboard.press('Enter');
  await panel.waitFor();
  await settle();
  await page.keyboard.press('Escape');
  await panel.waitFor({state:'detached'});
  await settle();
  assert.equal(await cost.evaluate(el => el === document.activeElement), true);
  checks.push('Cards animate into model totals without desktop layout movement; values have no hover tips; click and keyboard restore the overview');
  await startSwapAudit();
  await page.evaluate(async () => {
    for (let i=0;i<5;i++) {
      document.querySelector('.cost-toggle').click();
      for (let frame=0;frame<4;frame++) await new Promise(requestAnimationFrame);
    }
  });
  await panel.waitFor();
  await settle();
  await finishSwapAudit();
  assert.equal(await cost.getAttribute('aria-expanded'),'true');
  assert.equal(await page.locator('.token-card,.sessions-card,.cache-card').count(),0);
  await hide();
  checks.push('Opening, closing, and rapid repeated clicks never render outgoing cards over the cost card');
  for (const width of [1366,1101,800,390,320]) {
    await page.setViewportSize({width,height:844});
    await show();
    await bounds();
    if (width === 390) await page.screenshot({path:`${output}/models-mobile.png`});
    await hide();
  }
  await page.emulateMedia({reducedMotion:'reduce'});
  await show();
  assert.equal(await segments.first().evaluate(el => getComputedStyle(el).animationName), 'none');
  assert.equal(await ink.evaluate(el => getComputedStyle(el).animationName), 'none');
  await hide();
  checks.push('Both layouts fit desktop through 320px mobile; reduced-motion disables the perpetual border animation');
  models.push(...['extra-a','extra-b'].map(model => ({model,usage:usage(0),estimated_cost_usd:0,cost_complete:true})));
  await page.reload();
  await page.waitForFunction(() => document.querySelector('.cost-toggle')?.disabled === false);
  await show();
  assert.equal(await page.locator('.metrics .model-card').count(), 8);
  const grid = panel.locator('.model-totals');
  assert.equal(await grid.evaluate(el => el.scrollHeight > el.clientHeight), false);
  assert.equal(await grid.evaluate(el => getComputedStyle(el).overflowY), 'visible');
  for (const [width,height] of [[1536,960],[1536,844],[390,844]]) {
    await page.setViewportSize({width,height});
    for (const count of [6,7,8,9,10,12,24,36,60]) {
      models = Array.from({length:count}, (_, i) => ({model:`model-${i}`,usage:usage(1000000+i),estimated_cost_usd:i+1,cost_complete:true}));
      await page.reload();
      await page.waitForFunction(() => document.querySelector('.cost-toggle')?.disabled === false);
      const costHeight = (await page.locator('.cost-card').boundingBox()).height;
      await show();
      const cards = await page.locator('.metrics .model-card').evaluateAll(nodes => nodes.map(node => {
        const r = node.getBoundingClientRect(); return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};
      }));
      assert.equal(cards.length,count);
      const expectedHeight = width > 1100 ? Math.max(84,(costHeight-10)/2) : 84;
      assert.ok(cards.every(card => Math.abs(card.height-expectedHeight)<1), 'All cards share a height based on the cost card, independent of model count');
      const rows = cards.filter((card,index) => index === 0 || Math.abs(card.top-cards[index-1].top)>1);
      const expectedGap = width > 600 ? 10 : 8;
      for (let i=1;i<rows.length;i++) {
        assert.ok(Math.abs(rows[i].top-rows[i-1].bottom-expectedGap)<1, 'Every model row has the same gap, including the second-to-third row boundary');
      }
      const region = await page.locator('.metrics').boundingBox();
      assert.ok(cards.every(card => card.top >= region.y && card.bottom <= region.y+region.height+1), 'Every model fits the expanded region');
      assert.equal(await grid.evaluate(el => el.scrollHeight > el.clientHeight),false);
      assert.ok(Math.abs((await page.locator('.cost-card').boundingBox()).height-costHeight)<1);
      if (count > 6 && width > 1100) {
        assert.ok(Math.abs(cards[6].left-region.x)<1, 'Seventh model starts under the left edge of the cost card');
        assert.ok(Math.abs(cards[6].width-cards[0].width)<1, 'Upper and full-width model rows use equal card widths');
        assert.ok(cards[6].top >= (await page.locator('.cost-card').boundingBox()).y+costHeight, 'Full-width rows clear the cost card');
        if (count >= 11) assert.ok(Math.abs(cards[6].top-cards[10].top)<1, 'Five models fill each full-width row');
      }
      assert.deepEqual(await page.locator('.metrics .model-totals').evaluateAll(nodes => nodes.map(el => el.scrollHeight>el.clientHeight)), Array(count>6?2:1).fill(false));
      if (count === 60) {
        await page.locator('.metrics .model-card').last().scrollIntoViewIfNeeded();
        assert.ok(await page.locator('main').evaluate(el => el.scrollTop)>0, 'The page scrolls to the final model');
        const last = await page.locator('.metrics .model-card').last().boundingBox();
        assert.ok(last.y >= 0 && last.y + last.height <= page.viewportSize().height);
      }
      if (count === 9 && width === 1536) await page.screenshot({path:`${output}/all-models-expanded.png`});
    }
  }
  checks.push('6–60 models keep equal heights and uniform row gaps at both desktop heights and on mobile; models after the first six fill rows from the left, with full page access');
  models = [{model:'unpriced',usage:usage(100),estimated_cost_usd:null,cost_complete:false}];
  await page.reload();
  await page.waitForFunction(() => document.querySelector('.cost-toggle')?.disabled === false);
  await show();
  assert.ok((await panel.innerText()).includes('未知'));
  models=[];
  await page.reload();
  await page.waitForFunction(() => document.querySelector('.cost-toggle')?.disabled);
  assert.equal(await panel.count(),0);
  checks.push('Unpriced models stay unknown and empty data disables the card toggle');
  assert.deepEqual(errors,[]);
  console.log(JSON.stringify({passed:true,checks},null,2));
} catch(error) {
  await page.screenshot({path:`${output}/failure.png`});
  throw error;
} finally {
  await writeFile(`${output}/checks.json`,JSON.stringify({checks,errors},null,2));
  await browser.close();
}
