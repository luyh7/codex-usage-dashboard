// Real HTTP + Svelte rendering on 8765; synthetic remote 10k rows, local 10k events.
import {chromium} from '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs';
import {readdir,readFile,writeFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
const base='http://127.0.0.1:8765';
const cache=process.env.HOME+'/.cache/ms-playwright';const dir=(await readdir(cache)).filter(x=>/^chromium-\d+$/.test(x)).sort().at(-1);
const browser=await chromium.launch({executablePath:`${cache}/${dir}/chrome-linux64/chrome`,headless:true});
const report={scale:{remote_sessions:10000,local_sessions:1,local_events:10004},browser:await browser.version(),samples:15,warmups:2,method:'Input/click dispatched in the page, through real HTTP to rendered DOM plus one paint frame. Includes transport, JSON decode, server query/search and DOM layout. Warm filesystem/index; no concurrent benchmark workers.',measurements:{},checks:[]};
const stats=samples=>({p50_ms:[...samples].sort((a,b)=>a-b)[Math.floor(samples.length/2)],p95_ms:[...samples].sort((a,b)=>a-b)[Math.ceil(samples.length*.95)-1],samples_ms:samples});
try{
 const context=await browser.newContext({viewport:{width:1440,height:1000},locale:'en-US',timezoneId:'UTC'});await context.addInitScript(()=>localStorage.setItem('cousash:v2:language','"en"'));const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(String(e)));
 report.health=await(await fetch(base+'/api/health')).json();assert(report.health.implementation==='rust');
 await page.goto(base+'/?period=all&view=sessions&timezone=UTC');await page.locator('.sessions-table tbody tr').first().waitFor();
 // Each sample searches a different row, so unchanged content cannot satisfy it.
 const search=[];
 for(let i=0;i<17;i++){
  const id=String(i+10).padStart(6,'0');
  const elapsed=await page.evaluate(async id=>{
   const start=performance.now();const input=document.querySelector('input[type=search]');input.value='session-'+id;input.dispatchEvent(new Event('input',{bubbles:true}));
   while(!document.querySelector('.sessions-table .session-title')?.textContent.includes(id)){if(performance.now()-start>5000)throw Error('search timeout');await new Promise(requestAnimationFrame)}
   document.body.getBoundingClientRect();await new Promise(requestAnimationFrame);return performance.now()-start;
  },id);if(i>=2)search.push(elapsed);
 }
 report.measurements.search_10000=stats(search);
 await page.getByRole('searchbox').fill('');await page.waitForFunction(()=>document.querySelectorAll('.sessions-table tbody tr').length===100);
 await page.locator('section.browser-pane > .pager').getByRole('button',{name:'Next',exact:true}).click();await page.waitForFunction(()=>document.querySelector('section.browser-pane > .pager span')?.textContent.startsWith('101'));
 await page.locator('.sessions-table .session-title').first().click();await page.getByRole('alert').waitFor();assert((await page.locator('.detail-panel').innerText()).includes('does not contain'));await page.locator('.detail-panel').getByRole('button',{name:'Close',exact:true}).click();report.checks.push('cross-page selection and old summary-only archive detail message');
 await page.getByRole('button',{name:'Projects',exact:true}).click();await page.locator('.project-heading').first().waitFor();
 const expanded=[];
 for(let i=0;i<17;i++){
  const elapsed=await page.evaluate(async()=>{const heading=[...document.querySelectorAll('.project-heading')].find(e=>e.textContent.includes('project-'));const block=heading.closest('.project-block');if(heading.getAttribute('aria-expanded')==='true'){heading.click();await new Promise(requestAnimationFrame)}const start=performance.now();heading.click();while(!block.querySelector('.task-row')){if(performance.now()-start>5000)throw Error('expand timeout');await new Promise(requestAnimationFrame)}document.body.getBoundingClientRect();await new Promise(requestAnimationFrame);return performance.now()-start;});if(i>=2)expanded.push(elapsed);
 }
 report.measurements.project_expand_10000=stats(expanded);console.log(JSON.stringify(report.measurements));
 const block=page.locator('.project-block').filter({has:page.locator('.task-row')}).first();await block.locator('.gutter:not([disabled])').first().click();await block.locator('.agent-row').first().waitFor();assert(await block.locator('.agent-row').count()>=4);report.checks.push('parent aggregate and expandable own/child agent rows');
 await block.locator('.pager').getByRole('button',{name:'Next',exact:true}).click();await page.waitForFunction(()=>[...document.querySelectorAll('.project-content .pager span')].some(e=>e.textContent.startsWith('31')));report.checks.push('project task group paging');
 const reanchored=page.waitForResponse(r=>r.url().includes('/api/v2/project-tasks?')&&r.url().includes('offset=30')&&r.status()===200);await fetch(base+'/api/remotes/rename',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({device_short_code:'bulk-fixture',label:'Updated bulk fixture'})});await reanchored;await page.waitForFunction(()=>[...document.querySelectorAll('.project-content .pager span')].some(e=>e.textContent.startsWith('31')));report.checks.push('task page retained across publication refresh');
 // Session with long local timeline is isolated by search; no cached detail in adapter.
 await page.getByRole('searchbox').fill('long-session');await page.waitForFunction(()=>document.querySelectorAll('.project-block').length===1 && document.querySelector('.project-heading')?.textContent.includes('long'));if(await page.locator('.project-heading').first().getAttribute('aria-expanded')==='false')await page.locator('.project-heading').first().click();await page.locator('.task-row .session-title').first().waitFor();const details=[];
 for(let i=0;i<17;i++){
  const elapsed=await page.evaluate(async()=>{const start=performance.now();document.querySelector('.task-row .session-title').click();while(document.querySelectorAll('.event-scroll tbody tr').length!==100){if(performance.now()-start>5000)throw Error('detail timeout');await new Promise(requestAnimationFrame)}document.querySelector('.event-scroll').getBoundingClientRect();await new Promise(requestAnimationFrame);return performance.now()-start;});if(i>=2)details.push(elapsed);await page.locator('.detail-panel').getByRole('button',{name:'Close',exact:true}).click();
 }
 report.measurements.detail_10004=stats(details);
 // Retry after a concrete transport failure.
 await page.route('**/api/v2/projects?**',route=>route.fulfill({status:503,contentType:'application/json',body:'{"error":"synthetic unavailable"}'}),{times:1});await page.getByRole('searchbox').fill('failure-probe');await page.getByRole('alert').waitFor();assert((await page.getByRole('alert').innerText()).includes('synthetic unavailable'));await page.getByRole('button',{name:'Retry',exact:true}).click();await page.getByText('No sessions match these filters',{exact:true}).waitFor();report.checks.push('failed listing and successful retry');
 assert.deepEqual(errors,[]);report.gates=Object.fromEntries(Object.entries(report.measurements).map(([k,v])=>[k,v.p95_ms<100]));
 await writeFile('benchmarks/implementation/results/browser-performance.json',JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report));
}finally{await browser.close()}
