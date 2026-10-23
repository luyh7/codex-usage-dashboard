import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { installMintFixture, sessions, summary, stamp } from './fixtures/clear_mint.mjs';
import { ringArc } from '../web/src/lib/domain/ring.ts';

const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs');
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH || undefined, headless: true });
const context = await browser.newContext({ viewport: { width: 1536, height: 1000 }, locale: 'zh-CN', reducedMotion: 'no-preference' });
await installMintFixture(context);
await context.addInitScript(() => {
  const streams = new Set();
  window.EventSource = class extends EventTarget {
    constructor() { super(); streams.add(this); queueMicrotask(() => this.onopen?.(new Event('open'))); }
    close() { streams.delete(this); }
  };
  window.__publishRingRevision = revision => {
    for (const stream of streams) stream.dispatchEvent(new MessageEvent('revision', { data: JSON.stringify({ phase: 'ready', revision }) }));
  };
});
let revision = 1, main = 8, agents = 2, total = 10, cache = 89;
await context.route('**/api/v2/projects?*', async route => {
  const aggregate = { ...summary(sessions), main_count: main, agent_count: agents, session_count: total };
  aggregate.usage = { ...aggregate.usage, input_tokens: cache === null ? 0 : 1000, cached_input_tokens: cache === null ? 0 : cache * 10 };
  const items = ['web-app', 'data-tools', 'docs-site'].map(key => ({ key, label: key, ...summary(sessions.filter(row => row.project === key)), environments: ['Local'], updated_at: stamp }));
  const body = { page: { query_token: `rings-${revision}`, revision: `rings-${revision}`, items, summary: aggregate, total: items.length, offset: 0, limit: 100, next_cursor: null, previous_cursor: null }, facets: aggregate,
    period: { key: 'week', start_date: '2026-10-19', end_date: '2026-10-25' }, generated_at: stamp, codex_sources: [], remotes: [], current_device_short_code: 'this-device' };
  await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
});
const output = 'output/verification/soft-rings';
await mkdir(output, { recursive: true });
const page = await context.newPage();
page.setDefaultTimeout(10000);
const errors = [], checks = [];
page.on('pageerror', error => errors.push(String(error)));
const paths = { session: '.session-agent-segment', cache: '.cache-chart .ring-value' };
const publish = () => page.evaluate(revision => window.__publishRingRevision(revision), ++revision);
const settled = async () => {
  const agentPercent = main !== undefined && agents !== undefined && main + agents === total && total > 0 ? agents / total * 100 : null;
  const expected = { session: ringArc(agentPercent ?? 0, 100 - (agentPercent ?? 0)), cache: ringArc(cache ?? 0) };
  await page.waitForFunction(({ paths, expected }) => Object.entries(paths).every(([key, selector]) => document.querySelector(selector)?.getAttribute('d') === expected[key]), { paths, expected });
};
const observe = () => page.evaluate(() => {
  window.__ringObserver?.disconnect();
  window.__ringFrames = [];
  window.__ringObserver = new MutationObserver(() => window.__ringFrames.push({
    session: document.querySelector('.session-agent-segment').getAttribute('d'),
    cache: document.querySelector('.cache-chart .ring-value').getAttribute('d'),
    count: document.querySelector('.session-value .animated-value > [aria-hidden]').textContent,
  }));
  window.__ringObserver.observe(document.querySelector('.metrics'), { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ['d'] });
});
try {
  await page.goto('http://127.0.0.1:8765/?period=week');
  await page.locator('.session-agent-segment').waitFor();
  await settled();
  assert.equal(await page.locator('.session-main-segment').getAttribute('stroke-dasharray'), null, 'A full base circle prevents white seams');
  assert.equal(await page.locator('.session-main-segment').getAttribute('opacity'), '1');
  const initial = await page.locator('.session-agent-segment').getAttribute('d');
  await page.evaluate(() => { window.__originalSessionRing = document.querySelector('.session-ring'); });
  await observe();
  main = 13; agents = 7; total = 20; cache = 62.8;
  await publish();
  await settled();
  const trace = await page.evaluate(() => window.__ringFrames);
  for (const key of ['session', 'cache', 'count']) assert.ok(new Set(trace.map(frame => frame[key])).size > 3, `${key} must show intermediate update frames`);
  assert.notEqual(await page.locator('.session-agent-segment').getAttribute('d'), initial);
  assert.equal(await page.evaluate(() => window.__originalSessionRing === document.querySelector('.session-ring')), true, 'Live refresh preserves the ring component');
  checks.push('Session and cache arcs animate with headline numbers on real revision updates');

  main = 2; agents = 18; cache = 97.1;
  const previousCache = await page.locator('.cache-chart .ring-value').getAttribute('d');
  await publish();
  await page.waitForFunction(({ before, after }) => {
    const path = document.querySelector('.cache-chart .ring-value').getAttribute('d');
    return path !== before && path !== after;
  }, { before: previousCache, after: ringArc(cache) });
  main = 18; agents = 2; cache = 25;
  await publish();
  await settled();
  checks.push('An interrupted update settles at the newest proportions without replaying stale data');

  for (const [width, expectedWidth] of [[1920, 76], [1536, 61.44], [1280, 52], [1101, 52], [1024, 48], [760, 36], [390, 36], [320, 36]]) {
    await page.setViewportSize({ width, height: width > 1100 ? 1000 : 844 });
    const geometry = await page.locator('.metric-ring').evaluateAll(nodes => nodes.map(svg => ({ width: svg.getBoundingClientRect().width, box: svg.getAttribute('viewBox'), radius: svg.querySelector('circle').getAttribute('r'), strokes: [...svg.querySelectorAll('circle,path')].map(node => getComputedStyle(node).strokeWidth), cap: getComputedStyle(svg.querySelector('path')).strokeLinecap })));
    for (const ring of geometry) {
      assert.ok(Math.abs(ring.width - expectedWidth) < .1, JSON.stringify({ width, ring }));
      assert.equal(ring.box, '0 0 44 44'); assert.equal(ring.radius, '17');
      assert.ok(ring.strokes.every(stroke => stroke === '6px')); assert.equal(ring.cap, 'butt');
    }
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    await page.locator('.metrics').screenshot({ path: `${output}/cards-${width}.png` });
  }
  checks.push('All eight responsive sizes retain the previous diameter and six-unit stroke');

  await page.locator('.session-chart').focus();
  await page.getByRole('tooltip').waitFor();
  assert.match(await page.getByRole('tooltip').innerText(), /18 · 90%/);
  await page.keyboard.press('Escape');
  assert.equal(await page.getByRole('tooltip').count(), 0);
  await page.locator('.session-chart').blur();

  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await observe();
  main = 0; agents = 20; cache = 100;
  await publish(); await settled();
  const reduced = await page.evaluate(() => window.__ringFrames);
  assert.ok(new Set(reduced.map(frame => frame.session)).size <= 2);
  assert.equal((await page.locator('.session-agent-segment').getAttribute('d')).match(/ A /g).length, 2);
  for (const [nextMain, nextAgents, nextTotal, nextCache, opacity] of [[20, 0, 20, 0, '1'], [0, 0, 0, null, '0'], [undefined, undefined, 20, null, '0']]) {
    main = nextMain; agents = nextAgents; total = nextTotal; cache = nextCache;
    await publish(); await settled();
    assert.equal(await page.locator('.session-main-segment').getAttribute('opacity'), opacity);
    assert.equal(await page.locator('.session-agent-segment').getAttribute('d'), '');
    assert.equal(await page.locator('.cache-chart .ring-value').getAttribute('d'), '');
  }
  checks.push('Keyboard inspection, reduced motion, empty/unknown composition and 0/100% endpoints remain correct');
  assert.deepEqual(errors, []);
  await writeFile(`${output}/checks.json`, JSON.stringify({ passed: true, checks, errors }, null, 2));
  console.log('PASS: ' + checks.join('; '));
} finally { await browser.close(); }
