// Exercises the current native binary only, with synthetic logs and private
// runtime directories. Port 0 never replaces a user's existing listener.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync, execFileSync } from 'node:child_process';
import { mkdir, mkdtemp, writeFile, readFile, copyFile, rm, symlink, lstat } from 'node:fs/promises';
import { randomBytes, createHash } from 'node:crypto';
import { once } from 'node:events';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync, gunzipSync } from 'node:zlib';
import http from 'node:http';
import { binaryName, platformKey } from '../skills/codex-usage-dashboard/scripts/install_runtime.mjs';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const binary = path.resolve(process.env.AUDIT_BINARY || path.join(root, 'target/debug', binaryName()));
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
const rawStatus = (url, headers) => new Promise((resolve,reject) => {
  const request=http.get(url,{headers,agent:false},response=>{
    response.resume(); response.on('end',()=>resolve(response.statusCode));
  });
  request.on('error',reject); request.setTimeout(5000,()=>request.destroy(Error('HTTP timeout')));
});
async function fixture(t) {
  const parent = path.join(root, 'target/audit-tests');
  await mkdir(parent, {recursive:true});
  const dir = await mkdtemp(path.join(parent, 'case-'));
  const home = path.join(dir, 'codex');
  await mkdir(path.join(home, 'sessions'), {recursive:true});
  const env = {...process.env, CODEX_HOME:home, COUSASH_CONFIG_DIR:path.join(dir,'state'), COUSASH_SHUTDOWN_TOKEN:randomBytes(32).toString('hex'), COUSASH_INSTANCE_ID:randomBytes(16).toString('hex')};
  const fixture = {dir, home, env, stop:undefined};
  t.after(async () => {
    await fixture.stop?.();
    await rm(dir, {recursive:true, force:true});
  });
  return fixture;
}
async function server(t, f) {
  const child = spawn(binary, ['--serve','--port','0','--codex-home',f.home,'--no-auto-windows','--timezone','Asia/Shanghai'], {cwd:root,env:f.env,windowsHide:true,stdio:['ignore','pipe','pipe']});
  let logs = '';
  child.stdout.on('data', data => logs += data);
  child.stderr.on('data', data => logs += data);
  const exited = once(child, 'exit');
  let base;
  f.stop = async () => {
    if (child.exitCode === null) {
      if (base) await fetch(base+'/api/shutdown', {method:'POST',headers:{Authorization:'Bearer '+f.env.COUSASH_SHUTDOWN_TOKEN}}).catch(()=>{});
      await Promise.race([exited, pause(3000)]);
      if (child.exitCode === null) child.kill(); // Only this fixture's own child.
      await exited;
    }
  };
  for (let i=0;i<300;i++) {
    base = logs.match(/http:\/\/127\.0\.0\.1:\d+/)?.[0];
    if (base && (await (await fetch(base+'/api/health')).json()).index.phase === 'ready') return base;
    if (child.exitCode !== null) throw Error(logs);
    await pause(100);
  }
  throw Error('Fixture startup timeout: '+logs);
}

test('native HTTP boundary blocks hostile reads/writes while legitimate operations work', async t => {
  const f = await fixture(t);
  const rows = [
    {type:'session_meta',payload:{id:'audit-session',model:'gpt-5',cwd:path.join(f.dir,'private-project')}},
    {timestamp:'2026-10-01T00:00:00Z',type:'event_msg',payload:{type:'user_message',message:'AUDIT_PRIVATE_PROMPT_CANARY'}},
    {timestamp:'2026-10-01T00:00:01Z',type:'event_msg',payload:{type:'token_count',info:{total_token_usage:{input_tokens:100,output_tokens:20,total_tokens:120}}}},
  ];
  await writeFile(path.join(f.home,'sessions/rollout-audit.jsonl'), rows.map(JSON.stringify).join('\n')+'\n');
  const base = await server(t, f);
  const headers = {'Content-Type':'application/json'};
  const post = (route, body, extra={}) => fetch(base+route, {method:'POST',headers:{...headers,...extra},body:JSON.stringify(body)});
  for (const route of ['/api/health','/api/security/token','/api/export.json','/api/export.json.gz','/api/v2/projects','/api/revisions','/']) {
    for (const hostile of [{Host:'evil.test'}, {Origin:'https://evil.test'}, {Origin:'null'}, {Origin:base.replace(/:\d+$/,':1')}, {'Sec-Fetch-Site':'cross-site'}]) {
      assert.equal(await rawStatus(base+route,hostile),403,route+JSON.stringify(hostile));
    }
  }
  const session = await fetch(base+'/api/security/token', {headers:{Origin:base,'Sec-Fetch-Site':'same-origin'}});
  assert.equal(session.headers.get('cache-control'),'no-store');
  assert.equal(session.headers.get('x-frame-options'),'DENY');
  const token = (await session.json()).token;
  assert.match(token,/^[a-f0-9]{64}$/);
  for (const route of ['/api/refresh','/api/remotes/import','/api/remotes/import-file','/api/remotes/rename','/api/remotes/delete','/api/export/start']) {
    assert.equal((await post(route,{})).status,403);
    assert.equal((await post(route,{}, {'X-Codex-Write-Token':'wrong'})).status,403);
    assert.equal((await post(route,{}, {'X-Codex-Write-Token':token,'Content-Type':'text/plain'})).status,415);
    assert.equal((await post(route,{}, {'X-Codex-Write-Token':token, Origin:'https://evil.test'})).status,403);
  }
  headers['X-Codex-Write-Token'] = token;
  const exported = await (await fetch(base+'/api/export.json')).json();
  assert.equal(exported.snapshot.summary.usage.total_tokens,120);
  assert(JSON.stringify(exported).includes('AUDIT_PRIVATE_PROMPT_CANARY'));
  const shared = await (await fetch(base+'/api/export.json?mode=redacted')).json();
  const sharedText = JSON.stringify(shared);
  for (const secret of ['AUDIT_PRIVATE_PROMPT_CANARY','private-project',f.home,exported.device.short_code,'audit-session']) assert(!sharedText.includes(secret),secret);
  assert.equal(shared.snapshot.summary.usage.total_tokens,120);
  assert.equal(shared.snapshot.summary.estimated_cost_usd,exported.snapshot.summary.estimated_cost_usd);
  assert.equal((await fetch(base+'/api/export.json?mode=typo')).status,400);
  const fullJob = await (await post('/api/export/start',{})).json();
  const sharedJob = await (await post('/api/export/start?mode=redacted',{})).json();
  assert.notEqual(fullJob.id,sharedJob.id);
  assert.equal((await fetch(base+'/api/export.json.gz?job='+fullJob.id+'&mode=redacted')).status,404);
  for (let i=0;i<100;i++) {
    const job = await (await fetch(base+'/api/export/progress?id='+sharedJob.id)).json();
    if (job.phase === 'ready') break;
    assert.notEqual(job.phase,'error'); await pause(20);
  }
  const download = await fetch(base+'/api/export.json.gz?job='+sharedJob.id+'&mode=redacted');
  assert.equal(download.status,200);
  assert.match(download.headers.get('content-disposition'),/cousash-shared/);
  assert.deepEqual(JSON.parse(gunzipSync(Buffer.from(await download.arrayBuffer()))).snapshot,shared.snapshot);
  for (let i=0;i<2;i++) {
    const imported = await post('/api/remotes/import',{snapshot:shared,label:'Shared'});
    assert.equal(imported.status,200,await imported.text());
  }
  const stored = JSON.parse(await readFile(path.join(f.env.COUSASH_CONFIG_DIR,'remotes',shared.device.short_code+'.json'),'utf8'));
  assert.equal(stored.snapshot.sessions.length,1);
  assert.equal(stored.snapshot.summary.usage.total_tokens,120);
  assert.equal((await post('/api/refresh',{})).status,200);
  assert.equal((await (await fetch(base+'/api/health')).json()).index.phase,'ready');
  const cli = spawnSync(binary,['--export-snapshot','--redact','--codex-home',f.home,'--no-auto-windows'],{cwd:f.dir,env:f.env,encoding:'utf8',windowsHide:true,timeout:30000});
  assert.equal(cli.status,0,cli.stderr);
  const cliShared = JSON.parse(gunzipSync(await readFile(path.join(f.dir,'cousash-shared.json.gz'))));
  assert.equal(cliShared.device.short_code,shared.device.short_code);
  assert.deepEqual(cliShared.snapshot.sessions.map(s=>s.uid),shared.snapshot.sessions.map(s=>s.uid));
  exported.device.short_code = 'audit-remote'; exported.device.label = 'Audit remote';
  let result = await post('/api/remotes/import', {snapshot:exported,label:'Audit remote'});
  assert.equal(result.status,200,await result.text());
  assert.equal((await post('/api/remotes/rename',{device_short_code:'audit-remote',label:'Renamed'})).status,200);
  const remoteFile = path.join(f.env.COUSASH_CONFIG_DIR,'remotes','audit-remote.json');
  // Unauthorized deletion never changes the stored snapshot.
  const before = await readFile(remoteFile);
  assert.equal((await post('/api/remotes/delete',{device_short_code:'audit-remote',confirm:true},{'X-Codex-Write-Token':'wrong'})).status,403);
  assert.deepEqual(await readFile(remoteFile),before);
  assert.equal((await post('/api/remotes/delete',{device_short_code:'audit-remote',confirm:true})).status,200);
  result = await fetch(base+'/api/remotes/import-file?label=Audit', {method:'POST',headers:{'Content-Type':'application/octet-stream','X-Codex-Write-Token':token},body:gzipSync(JSON.stringify(exported))});
  assert.equal(result.status,200,await result.text());
  result = await post('/api/export/start',{});
  assert.equal(result.status,200);
  assert.equal((await fetch(base+'/api/shutdown',{method:'POST'})).status,403);
  assert.equal((await fetch(base+'/api/shutdown',{method:'POST',headers:{Authorization:'Bearer '+token}})).status,403);
  const info = JSON.parse(execFileSync(binary,['--build-info'],{encoding:'utf8',windowsHide:true}));
  assert.equal((await (await fetch(base+'/api/health')).json()).build_id,info.build_id);
});

test('non-loopback listeners fail before opening runtime state', async t => {
  const f = await fixture(t);
  for (const host of ['0.0.0.0','192.168.1.1','::']) {
    const result=spawnSync(binary,['--serve','--port','0','--host',host],{env:f.env,encoding:'utf8',windowsHide:true,timeout:5000});
    assert.notEqual(result.status,0); assert.match(result.stderr,/only loopback/);
  }
});

test('real offline installer preserves modified legacy directories, links and obsolete files', async t => {
  const f=await fixture(t);
  const legacy=path.join(f.home,'skills/cousash-open');
  const outside=path.join(f.dir,'user-owned');
  const scripts=path.join(f.home,'skills/codex-usage-dashboard/scripts');
  await mkdir(legacy,{recursive:true}); await mkdir(outside); await mkdir(path.join(scripts,'__pycache__'),{recursive:true});
  await writeFile(path.join(legacy,'custom.txt'),'USER MODIFICATIONS');
  await writeFile(path.join(outside,'notes.txt'),'USER OWNED');
  await writeFile(path.join(scripts,'open_dashboard.py'),'USER MODIFIED OBSOLETE FILE');
  await writeFile(path.join(scripts,'__pycache__/notes.txt'),'USER OWNED CACHE NOTES');
  const link=path.join(f.home,'skills/cousash-export');
  await symlink(outside,link,process.platform==='win32'?'junction':'dir');
  const info=JSON.parse(execFileSync(binary,['--build-info'],{encoding:'utf8',windowsHide:true}));
  const sha256=createHash('sha256').update(await readFile(binary)).digest('hex');
  const file=binaryName(); await copyFile(binary,path.join(f.dir,file));
  const manifest=path.join(f.dir,'manifest.json');
  await writeFile(manifest,JSON.stringify({version:info.version,protocol_version:1,ui:'svelte',platforms:{[platformKey()]:{file,sha256,build_id:info.build_id}}}));
  const result=spawnSync(process.execPath,[path.join(root,'bin/install.js'),'--runtime-manifest',manifest,'--offline'],{cwd:root,env:f.env,encoding:'utf8',windowsHide:true,timeout:30000});
  assert.equal(result.status,0,result.stderr);
  assert.match(result.stderr,/Preserved legacy skill/);
  assert.equal(await readFile(path.join(legacy,'custom.txt'),'utf8'),'USER MODIFICATIONS');
  assert((await lstat(link)).isSymbolicLink());
  assert.equal(await readFile(path.join(outside,'notes.txt'),'utf8'),'USER OWNED');
  assert.equal(await readFile(path.join(scripts,'open_dashboard.py'),'utf8'),'USER MODIFIED OBSOLETE FILE');
  assert.equal(await readFile(path.join(scripts,'__pycache__/notes.txt'),'utf8'),'USER OWNED CACHE NOTES');
  if (process.platform === 'win32') {
    // A fresh installation with no PowerShell available must report the optional
    // shortcut failure. It must not call --stop against an existing runtime.
    const failed=spawnSync(process.execPath,[path.join(root,'bin/install.js'),'--runtime-manifest',manifest,'--offline','--shortcut'],{
      cwd:root,env:{...f.env,CODEX_HOME:path.join(f.dir,'fresh-codex'),PATH:path.join(f.dir,'no-programs'),USERPROFILE:f.dir,HOME:f.dir},
      encoding:'utf8',windowsHide:true,timeout:30000,
    });
    assert.notEqual(failed.status,0);
    assert.match(failed.stderr,/Installation failed/);
  }
});

test('native Windows shortcut preserves dollar expressions and quotes literally', {skip:process.platform!=='win32'}, async t => {
  const f=await fixture(t);
  const folder=path.join(f.dir,"Audit$(1+1)","O'Brien");
  await mkdir(folder,{recursive:true});
  await mkdir(path.join(f.dir,'Desktop'));
  const executable=path.join(folder,binaryName()); await copyFile(binary,executable);
  execFileSync(executable,['--shortcut'],{env:{...f.env,HOME:f.dir,USERPROFILE:f.dir},windowsHide:true});
  const shortcut=path.join(f.dir,'Desktop/Codex Usage Dashboard.lnk');
  const quote=value=>"'"+value.replaceAll("'","''")+"'";
  const script=`$s=(New-Object -ComObject WScript.Shell).CreateShortcut(${quote(shortcut)}); [Console]::Write($s.TargetPath)`;
  const actual=execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',script],{encoding:'utf8',windowsHide:true});
  assert.equal(actual.toLowerCase(),executable.toLowerCase());
});
