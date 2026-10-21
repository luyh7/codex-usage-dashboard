// Production HTML is intercepted in the browser at port 8765. No second server.
import { chromium } from 'playwright';
import { readFile, writeFile, mkdir, readdir } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';

const root = new URL('../../../', import.meta.url).pathname;
const here = new URL('.', import.meta.url).pathname;
const source = root + 'skills/codex-usage-dashboard/scripts/codex_usage_dashboard.py';
const html = execFileSync('python3', ['-c', `import ast,pathlib
for n in ast.parse(pathlib.Path(${JSON.stringify(source)}).read_text()).body:
 if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='HTML' for t in n.targets):print(ast.literal_eval(n.value),end='')`], { maxBuffer: 4e6 }).toString();
const script = await readFile(here + 'dist/probe.js', 'utf8');
const hook = `
window.probe = {
 state, renderAll, renderTable, renderMetrics, renderDetails, drawTimeline, renderCalendar,
 renderRemoteModal, baseFilteredSessions, summarizeFilteredSessions, projectGroups, taskGroupsForRows,
 updatePeriodButtons, updateViewButtons, bindTableInteractions,
 setData(payload) {
  state.sessions = payload.sessions; state.summary=payload.summary;
  state.search=''; state.environment='all'; state.source='all'; state.model='all';
  state.viewMode='project';state.selectedUid=null;state.projectExpanded={};state.projectShowAll={};state.taskExpanded={};
  state.dailyUsage=Array.from({length:730},(_,i)=>({date:dateKey(new Date(2025,0,1+i)),usage:{total_tokens:i*1234}}));
  state.dailyUsageComplete=true;state.calendarOpen=false;state.calendarMonth='2026-10';state.calendarView='days';
  state.codexSources=[];applyStaticText();populateEnvironmentFilter();populateSourceFilter();populateModelFilter();setLoadingIndicator(false);
 },
 blocks() {
  const groups=projectGroups();
  document.getElementById('resultCount').textContent=t('projectRowCount',{projects:fmt(groups.length),sessions:fmt(groups.reduce((n,g)=>n+g.rows.length,0))});
  return groups.map(group=>{
   const expanded=isProjectExpanded(group),all=isProjectShowingAll(group),tasks=taskGroupsForRows(group.rows);
   const visible=expanded?(all?tasks:tasks.slice(0,projectPreviewLimit)):[];
   return {key:group.key, html:[projectHeaderHtml(group),...visible.map(task=>taskGroupRowsHtml(task,'project-session-row',{compactProject:true})),expanded&&tasks.length>projectPreviewLimit?projectMoreRowHtml(group,tasks.length-projectPreviewLimit):''].join('')};
  });
 },
 semantic() {return {uids:baseFilteredSessions().map(r=>r.uid),summary:summarizeFilteredSessions(),groups:projectGroups().map(g=>({key:g.key,usage:g.usage,rows:g.rows.map(r=>r.uid)}))};},
 haystack(row) {return [row.title,row.session_id,modelLabel(row),row.project,row.project_root,row.workspace_root,row.project_branch,row.cwd,row.path,row.source,row.environment].join(' ').toLowerCase();},
 detail(count){
  const row=state.sessions[0];let timeline=Array.from({length:count},(_,i)=>({timestamp:new Date(Date.UTC(2026,9,1,0,i)).toISOString(),model:row.model,service_tier:'',total_token_usage:{input_tokens:(i+1)*1000,cached_input_tokens:(i+1)*600,output_tokens:(i+1)*120,total_tokens:(i+1)*1120},last_token_usage:{input_tokens:1000,cached_input_tokens:600,output_tokens:120,total_tokens:1120}}));
  return {...row,timeline,token_event_count:count,tool_counts:{exec_command:20},applied_price_segments:[]};
 }
};
`;
const boot = html.lastIndexOf('    applyStaticText();\n    populateEnvironmentFilter();');
assert(boot > 0);
const end = html.indexOf('  </script>', boot);
const intercepted = html.slice(0, boot) + hook + html.slice(end);
const cache = process.env.HOME + '/.cache/ms-playwright';
const dirs = await readdir(cache);
const chromiumDir = dirs.filter(x=>/^chromium-\d+$/.test(x)).sort().at(-1);
const browser = await chromium.launch({ executablePath: cache + '/' + chromiumDir + '/chrome-linux64/chrome', headless:true });
const results=[];
const report={ environment:{browser:await browser.version(),viewport:{width:1440,height:1000},timezone:'Asia/Shanghai',svelte:JSON.parse(await readFile(here+'node_modules/svelte/package.json','utf8')).version,source_sha256:createHash('sha256').update(await readFile(source)).digest('hex')},
 method:{samples:15,warmups:2,network:'all dashboard requests intercepted, synthetic data only',timing:'JS + DOM update + forced layout; next requestAnimationFrame callback delay separately recorded (not a paint-completion measurement)',framework:'Svelte keyed HTML probe shares exact production grouping/markup; not a full Svelte app rewrite',unchanged:'forced render microbenchmark; current production already skips rendering on HTTP 204',candidate_bundle_bytes:Buffer.byteLength(script)},results};
await mkdir(here+'../results',{recursive:true});
const save=()=>writeFile(here+'../results/frontend.json',JSON.stringify(report,null,2));
const stats=values=>({median_ms:[...values].sort((a,b)=>a-b)[Math.floor(values.length/2)],p95_ms:[...values].sort((a,b)=>a-b)[Math.ceil(values.length*.95)-1],samples_ms:values});

async function pageFor(size,variant){
 const context=await browser.newContext({viewport:report.environment.viewport,timezoneId:'Asia/Shanghai'});
 const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.route('http://127.0.0.1:8765/**',route=>route.request().url().endsWith('/')?route.fulfill({contentType:'text/html',body:intercepted}):route.fulfill({status:404,body:'probe blocks network'}));
 await page.goto('http://127.0.0.1:8765/');
 const payload=JSON.parse(await readFile(here+`../.work/frontend-${size}.json`,'utf8'));
 await page.evaluate(p=>probe.setData(p),payload);
 if(variant==='svelte')await page.addScriptTag({content:script});
 await page.evaluate(async variant=>{
  const b=window.probe;window.variant=variant;
  let update;
  if(variant==='svelte')update=window.mountSvelteProbe();
  if(variant==='keyed_dom'){
   const cache=new Map();const target=document.getElementById('sessionRows');target.replaceChildren();
   update=async blocks=>{
    const live=new Set(blocks.map(x=>x.key));for(const [key,entry] of cache){if(!live.has(key)){entry.nodes.forEach(n=>n.remove());cache.delete(key);}}
    let cursor=target.firstChild;
    for(const block of blocks){let entry=cache.get(block.key);
     if(!entry||entry.html!==block.html){const temp=document.createElement('tbody');temp.innerHTML=block.html;const nodes=[...temp.childNodes];
      if(entry){if(entry.nodes.includes(cursor))cursor=entry.nodes.at(-1).nextSibling;entry.nodes.forEach(n=>n.remove());}
      entry={html:block.html,nodes};cache.set(block.key,entry);
     }
     for(const node of entry.nodes){if(node!==cursor)target.insertBefore(node,cursor);cursor=node.nextSibling;}
    }
   };
  }
  window.refresh=async()=>{
   if(variant==='legacy'){b.renderAll();return;}
   b.renderMetrics();b.updatePeriodButtons();b.updateViewButtons();document.getElementById('limitSelect').disabled=true;await update(b.blocks());
  };
  // Candidates use one delegated listener instead of rebinding every unchanged row.
  if(variant!=='legacy')document.getElementById('sessionRows').addEventListener('click',async event=>{
   const project=event.target.closest('[data-project-toggle]');const task=event.target.closest('[data-task-toggle],[data-task-toggle-row]');
   if(project)b.state.projectExpanded[project.dataset.projectToggle]=b.state.projectExpanded[project.dataset.projectToggle]===false;
   else if(task){const key=task.dataset.taskToggle||task.dataset.taskToggleRow;b.state.taskExpanded[key]=b.state.taskExpanded[key]!==true;}
   else return;
   await window.refresh();
  });
  const initialStart=performance.now();await window.refresh();void document.getElementById('sessionRows').offsetHeight;window.initialRenderMs=performance.now()-initialStart;
 },variant);
 assert.equal(errors.length,0,errors.join('\n'));
 return {page,context,errors};
}

try{
 for(const size of [1000,10000]){
  const semantic=[];const rendered=[];const finalSemantic=[];
  for(const variant of ['legacy','keyed_dom','svelte']){
   const {page,context,errors}=await pageFor(size,variant);
   results.push({module:'initial_render',size,variant,single_sample_ms:await page.evaluate(()=>window.initialRenderMs)});
   semantic.push(await page.evaluate(()=>probe.semantic()));
   rendered.push(await page.locator('#sessionRows').innerText());
   for(const scenario of ['unchanged','search','one_session_update','project_toggle','agent_toggle']){
    await page.evaluate(()=>{probe.state.search='';probe.state.projectExpanded={};probe.state.taskExpanded={};});
    const measured=await page.evaluate(async scenario=>{
     const sync=[],frames=[];const b=probe;const firstGroup=b.projectGroups()[0];const task=b.taskGroupsForRows(firstGroup.rows)[0];
     for(let i=0;i<17;i++){
      if(scenario==='search')b.state.search=i%2?'project-01':'';
      if(scenario==='one_session_update')b.state.sessions[0].total_token_usage.total_tokens+=1;
      if(scenario==='project_toggle')b.state.projectExpanded[firstGroup.key]=i%2===0;
      if(scenario==='agent_toggle')b.state.taskExpanded[task.key]=i%2===0;
      const t=performance.now();await window.refresh();void document.getElementById('sessionRows').offsetHeight;const elapsed=performance.now()-t;
      await new Promise(requestAnimationFrame);const frame=performance.now()-t;
      if(i>=2){sync.push(elapsed);frames.push(frame);}
     }
     return {sync,frames,rows:document.querySelectorAll('#sessionRows tr').length,domNodes:document.querySelectorAll('*').length};
    },scenario);
    const row={module:'table',size,variant,scenario,...stats(measured.sync),frame:stats(measured.frames),rows:measured.rows,dom_nodes:measured.domNodes};results.push(row);await save();
    console.log(`${size} ${variant} ${scenario}: ${row.median_ms.toFixed(2)}ms`);
   }
   assert.equal(errors.length,0,errors.join('\n'));
   finalSemantic.push(await page.evaluate(()=>probe.semantic()));
   const cdp=await context.newCDPSession(page);await cdp.send('HeapProfiler.collectGarbage');
   const heap=await cdp.send('Runtime.getHeapUsage');results.push({module:'browser_heap',size,variant,used_bytes:heap.usedSize,backing_storage_bytes:heap.backingStorageSize,method:'after measured operations and forced GC; JS heap only'});
   // Actual click smoke test covers candidate event delegation.
   await page.evaluate(async()=>{probe.state.search='';probe.state.projectExpanded={};await window.refresh();});
   const before=await page.locator('#sessionRows tr').count();
   await page.locator('[data-project-toggle]').first().click();
   await page.waitForFunction(n=>document.querySelectorAll('#sessionRows tr').length<n,before);
   if(variant==='svelte'&&size===10000)await page.screenshot({path:here+'../results/svelte-probe.png'});
   await context.close();
  }
  assert.deepEqual(semantic[0],semantic[1]);assert.deepEqual(semantic[0],semantic[2]);
  assert.deepEqual(finalSemantic[0],finalSemantic[1]);assert.deepEqual(finalSemantic[0],finalSemantic[2]);
  // Relative timestamps may tick over between independent pages; numeric columns and titles must agree.
  const normalize=text=>text.replace(/\d+\s*(分钟|小时|天|秒|min|hr|day|sec)/g,'TIME');
  assert.equal(normalize(rendered[0]),normalize(rendered[1]));assert.equal(normalize(rendered[0]),normalize(rendered[2]));
  report[`table_parity_${size}`]='group order, session identities, aggregate values, visible text, project click passed';await save();
 }

 const {page,context,errors}=await pageFor(10000,'legacy');
 const modules=await page.evaluate(async()=>{
  const results=[];const measure=(name,fn,count=15)=>{const times=[];let value;for(let i=0;i<count+2;i++){const t=performance.now();value=fn();void document.body.offsetHeight;if(i>=2)times.push(performance.now()-t);}results.push({name,samples_ms:times});return value;};
  const b=probe;b.state.search='project-01';
  const expected=b.baseFilteredSessions().map(r=>r.uid);
  const index=measure('search_index_build',()=>b.state.sessions.map(r=>({row:r,text:b.haystack(r)})));
  const legacy=measure('search_current',()=>b.baseFilteredSessions().map(r=>r.uid));
  const candidate=measure('search_preindexed',()=>index.filter(x=>x.text.includes(b.state.search)).map(x=>x.row.uid));
  if(JSON.stringify(expected)!==JSON.stringify(candidate))throw Error('search parity');
  b.state.search='';
  measure('project_grouping_current',()=>b.projectGroups());
  measure('summary_current',()=>b.summarizeFilteredSessions());
  b.state.calendarOpen=true;
  for(const view of ['days','months','years']){b.state.calendarView=view;measure('calendar_'+view,()=>b.renderCalendar());}
  b.state.calendarOpen=false;
  b.state.remotes=Array.from({length:20},(_,i)=>({device_short_code:'synthetic-'+i,label:'Device '+i,session_count:500,usage:{total_tokens:i*1000},exported_at:'2026-10-21T00:00:00Z'}));
  measure('remote_dialog_20',()=>b.renderRemoteModal());
  for(const size of [1000,10000]){
   const detail=b.detail(size);
   measure('detail_full_'+size,()=>b.renderDetails(detail),7);
   const fullRows=document.querySelectorAll('.timeline-table tbody tr').length;
   const paged={...detail,timeline:detail.timeline.slice(-100)};
   // Keep original curve and metadata; only the event table is paginated.
   measure('detail_page100_'+size,()=>{b.renderDetails(paged);b.drawTimeline(detail.timeline);},7);
   const pageRows=document.querySelectorAll('.timeline-table tbody tr').length;
   if(fullRows!==size||pageRows!==100)throw Error('detail row count');
   const expected=detail.timeline.slice(-100).reverse().map(r=>r.last_token_usage.total_tokens);
   const actual=[...document.querySelectorAll('.timeline-table tbody tr')].map(r=>Number(r.cells[2].textContent.replace(/,/g,'')));
   if(JSON.stringify(expected)!==JSON.stringify(actual))throw Error('detail page parity');
   measure('chart_'+size,()=>b.drawTimeline(detail.timeline));
  }
  return results;
 });
 for(const m of modules)results.push({module:'isolated_ui',name:m.name,...stats(m.samples_ms)});
 assert.equal(errors.length,0,errors.join('\n'));report.isolated_parity='search IDs and detail newest-100 token values passed; other isolated modules baseline only';
 await context.close();
 await save();
}finally{await browser.close();}
