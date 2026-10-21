import { chromium } from '../benchmarks/refactor/frontend/node_modules/playwright/index.mjs';
import { readFile, writeFile, readdir, mkdir } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';

const origin = 'http://127.0.0.1:8765';
const cache = process.env.HOME + '/.cache/ms-playwright';
const dir = (await readdir(cache)).filter(x => /^chromium-\d+$/.test(x)).sort().at(-1);
const browser = await chromium.launch({executablePath:`${cache}/${dir}/chrome-linux64/chrome`,headless:true});
const report = {stage:1, data:'synthetic only', port:8765, browser:await browser.version(), platform:execFileSync('uname',['-sr']).toString().trim(), timezone:'Asia/Shanghai', viewport:{width:1440,height:1000}, samples:15, warmups:2, results:[], checks:[], unavailable:['Windows native','macOS']};
const source = 'web/classic/index.html';
report.source_sha256 = createHash('sha256').update(await readFile(source)).digest('hex');
const stats = values => ({p50_ms:[...values].sort((a,b)=>a-b)[Math.floor(values.length/2)],p95_ms:[...values].sort((a,b)=>a-b)[Math.ceil(values.length*.95)-1],samples_ms:values});
try {
 const context = await browser.newContext({viewport:report.viewport,timezoneId:report.timezone});
 const page = await context.newPage(); const errors=[];
 page.on('pageerror',e=>errors.push(String(e)));
 await page.goto(origin);
 await page.locator('[data-period-button="all"]').click();
 await page.locator('#eventRows tr').first().waitFor();
 assert.equal(await page.locator('#eventRows tr').count(),100);
 const health=await (await context.request.get(origin+'/api/health')).json();
 assert.equal(health.implementation,'rust'); report.build_id=health.build_id;
 const list=await (await context.request.get(origin+'/api/sessions?period=all')).json();
 const uid=list.sessions[0].uid, token=list.snapshot_token;
 const params=new URLSearchParams({id:uid,snapshot_token:token});
 const legacy=await (await context.request.get(origin+'/api/session?'+params)).json();
 const exported=await (await context.request.get(origin+'/api/export.json')).json();
 assert.deepEqual(exported.snapshot.details_by_uid[uid].timeline,legacy.timeline);
 let cursor='', events=[];
 do {
  const p=new URLSearchParams(params); if(cursor)p.set('cursor',cursor);
  const data=await (await context.request.get(origin+'/api/v2/events?'+p)).json();
  events.push(...data.events); cursor=data.next_cursor;
 } while(cursor);
 assert.deepEqual(events,[...legacy.timeline].sort((a,b)=>new Date(b.timestamp)-new Date(a.timestamp)));
 report.checks.push('10003 events: every cursor page equals full legacy detail and v1 export');
 const first=await page.locator('#eventRows tr:first-child td:first-child').getAttribute('title');
 await page.locator('#nextEvents').click();
 await page.waitForFunction(()=>document.querySelector('#eventPageStatus').textContent.startsWith('101'));
 assert.notEqual(await page.locator('#eventRows tr:first-child td:first-child').getAttribute('title'),first);
 await page.locator('#previousEvents').click();
 await page.waitForFunction(()=>document.querySelector('#eventPageStatus').textContent.startsWith('1–'));
 assert.equal(await page.locator('#eventRows tr:first-child td:first-child').getAttribute('title'),first);
 const marker=await page.locator('#sessionRows tr[data-uid]').evaluate(el=>{el.dataset.sentinel='retained';return el.dataset.uid});
 await page.locator('#sessionRows tr[data-uid]').focus();
 await page.keyboard.press('Enter');
 assert.equal(await page.locator(`[data-uid="${marker}"]`).getAttribute('data-sentinel'),'retained');
 const project=page.locator('[data-project-toggle]').first();
 await project.focus(); await page.keyboard.press('Enter');
 assert.equal(await page.locator('#sessionRows tr[data-uid]').count(),0);
 assert.equal(await page.evaluate(()=>document.activeElement.hasAttribute('data-project-toggle')),true);
 await page.keyboard.press('Enter');
 assert.equal(await page.locator('#sessionRows tr[data-uid]').count(),1);
 report.checks.push('real page: event next/previous, selection node preservation, keyboard project expand/collapse and focus');
 // Four clients, no-change response, remote CRUD, custom date boundaries.
 const readers=await Promise.all(Array.from({length:4},()=>context.request.get(origin+'/api/sessions?period=all')));
 for(const response of readers)assert.equal((await response.json()).snapshot_token,token);
 assert.equal((await context.request.get(origin+'/api/sessions?'+new URLSearchParams({period:'all',snapshot_token:token}))).status(),204);
 const outside=await (await context.request.get(origin+'/api/sessions?period=custom&start=2026-10-02&end=2026-10-02')).json();
 assert.equal(outside.sessions.length,0);
 exported.device.short_code='synthetic-stage1-remote';
 const post=async(path,data)=>(await context.request.post(origin+path,{data})).json();
 assert.equal((await post('/api/remotes/import',{snapshot:exported,label:'Synthetic remote'})).ok,true);
 assert.equal((await post('/api/remotes/rename',{device_short_code:exported.device.short_code,label:'Renamed'})).ok,true);
 assert.equal((await post('/api/remotes/delete',{device_short_code:exported.device.short_code,confirm:true})).ok,true);
 assert.equal((await (await context.request.get(origin+'/api/sessions?period=all')).json()).sessions.length,1);
 assert.equal((await context.request.get(origin+'/api/v2/events?'+new URLSearchParams({id:uid,snapshot_token:'stale'}))).status(),409);
 report.checks.push('HTTP: 4 readers, 204, date exclusion, remote import/rename/delete, stale revision');
 assert.deepEqual(errors,[]);
 await context.close();
 // Controlled DOM work benchmarks: same synthetic 10k payload, viewport and
 // full chart. Data loading / HTTP are excluded, recorded separately above.
 const fixture=JSON.parse(await readFile('benchmarks/refactor/.work/frontend-10000.json','utf8'));
 const legacyHtml=execFileSync('git',['show','c42f0e8ae948d4cde392fafa9894681a1200b17d:skills/codex-usage-dashboard/scripts/codex_usage_dashboard.py'],{maxBuffer:4e6}).toString().split('HTML = r"""')[1].split('"""')[0];
 const currentHtml=await readFile(source,'utf8');
 for(const variant of ['legacy','current']) {
  const html=variant==='legacy'?legacyHtml:currentHtml;
  const boot=html.lastIndexOf('    applyStaticText();\n    populateEnvironmentFilter();');
  assert(boot>0);
  const intercepted=html.slice(0,boot)+html.slice(html.indexOf('  </script>',boot));
  const ctx=await browser.newContext({viewport:report.viewport,timezoneId:report.timezone});
  const p=await ctx.newPage(); const faults=[]; p.on('pageerror',e=>faults.push(String(e)));
  await p.route(origin+'/**',route=>route.request().url()===origin+'/'?route.fulfill({contentType:'text/html',body:intercepted}):route.fulfill({status:404}));
  await p.goto(origin);
  await p.evaluate(payload=>{state.sessions=payload.sessions;state.summary=payload.summary;state.period='all';applyStaticText();renderAll();},fixture);
  const semantic=await p.evaluate(()=>({ids:baseFilteredSessions().map(r=>r.uid),summary:summarizeFilteredSessions()}));
  if(variant==='legacy') report.expected=semantic; else assert.deepEqual(semantic,report.expected);
  for (const operation of ['search','project_toggle','agent_toggle','detail_10003']) {
   const times=[];
   for(let i=0;i<17;i++) {
    const ms=await p.evaluate(({operation,i,detail,variant})=>{
     const start=performance.now();
     if(operation==='search'){state.search=i%2?'benchmark':'0001';renderAll();}
     if(operation==='project_toggle'){state.search='';const key=projectGroups()[0].key;state.projectExpanded[key]=state.projectExpanded[key]===false;renderAll();}
     if(operation==='agent_toggle'){const key=taskGroupsForRows(baseFilteredSessions())[0].key;toggleTaskGroup(key);}
     if(operation==='detail_10003'){
      if(variant==='current')detail.eventPage={events:[...detail.timeline].reverse().slice(0,100),total:detail.timeline.length,offset:0,limit:100,next_cursor:'unused',previous_cursor:null};
      renderDetails(detail);
     }
     void document.body.offsetHeight;
     return performance.now()-start;
    },{operation,i,detail:operation==='detail_10003'?legacy:null,variant});
    if(i>=2)times.push(ms);
   }
   report.results.push({variant,operation,...stats(times)});
  }
  // Combined filtering equals the same explicit legacy predicate.
  await p.evaluate(()=>{state.search='benchmark';state.source='active';state.environment='native';state.model='gpt-5';});
  const filtered=await p.evaluate(()=>baseFilteredSessions().map(row=>row.uid));
  const expected=fixture.sessions.filter(r=>r.source==='active'&&r.environment_id==='native'&&r.models.includes('gpt-5')).map(r=>r.uid);
  assert.deepEqual(filtered,expected);
  assert.deepEqual(faults,[]);
  await ctx.close();
 }
 delete report.expected;
 report.checks.push('10k synthetic sessions: combined filters and exact legacy summary parity');
 report.method='DOM + forced layout, 2 warmups and 15 samples; no network or data decode in render timing. Full chart retained. Algorithm/workload changes, no Rust acceleration claim.';
 report.status='passed';
 await mkdir('benchmarks/implementation/results',{recursive:true});
 await writeFile('benchmarks/implementation/results/stage1.json',JSON.stringify(report,null,2)+'\n');
 console.log(JSON.stringify({status:report.status,checks:report.checks,results:report.results.map(({samples_ms,...row})=>row)},null,2));
} finally {await browser.close();}
