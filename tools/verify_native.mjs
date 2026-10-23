#!/usr/bin/env node
// Fresh installation and real native lifecycle, on 8765, with no Python on PATH.
import assert from 'node:assert/strict';
import {execFileSync,spawn} from 'node:child_process';
import {mkdtemp,mkdir,readFile,writeFile,rm,copyFile,readdir,chmod,realpath,symlink} from 'node:fs/promises';
import http from 'node:http';
import net from 'node:net';
import path from 'node:path';
import os from 'node:os';
import {gunzipSync} from 'node:zlib';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {binaryName,platformKey} from '../skills/codex-usage-dashboard/scripts/install_runtime.mjs';
const root=path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const worktree=path.join(process.env.CARGO_TARGET_DIR || path.join(root,'target'),'release',binaryName());
const base='http://127.0.0.1:8765';
const replace=process.argv.includes('--replace-worktree');
const occupied=()=>new Promise(resolve=>{const socket=net.connect(8765,'127.0.0.1');socket.once('connect',()=>{socket.destroy();resolve(true)});socket.once('error',()=>resolve(false))});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const until=async test=>{const end=Date.now()+60000;while(!await test()){if(Date.now()>end)throw Error('Timed out');await sleep(50)}};
if(replace) execFileSync(worktree,['--stop']);
assert(!await occupied(),'8765 must be free; existing process was not stopped');
const tmp=await realpath(await mkdtemp(path.join(os.tmpdir(),'native-only-')));
const codex=path.join(tmp,'codex');await mkdir(path.join(codex,'sessions'),{recursive:true});
const config=path.join(tmp,'config');
const emptyPath=path.join(tmp,'no-interpreters');await mkdir(emptyPath);
const env={...process.env,PATH:emptyPath,CODEX_HOME:codex,COUSASH_CONFIG_DIR:config,TZ:'UTC'};
const skill=path.join(codex,'skills/codex-usage-dashboard');
const runtime=path.join(skill,'runtime');
const binary=path.join(runtime,binaryName());
const install=path.join(root,'bin/install.js');
const manifestPath=path.join(root,'dist/release/manifest.json');
const manifest=JSON.parse(await readFile(manifestPath,'utf8'));
const entry=manifest.platforms[platformKey()];
const run=(exe,args=[],ok=true)=>{
 try{return execFileSync(exe,args,{env,cwd:tmp,encoding:'utf8',timeout:70000,windowsHide:true})}
 catch(e){if(ok)throw Error(`${e.message}\n${e.stdout||''}\n${e.stderr||''}`);return null}
};
const installArgs=[install,'--runtime-manifest',manifestPath,'--offline'];
const launchArgs=['--no-browser','--codex-home',codex,'--no-auto-windows','--timezone','UTC'];
// Synchronous launcher calls can close a server before Node processes pooled
// socket EOF events. Probe each new process using a fresh, unpooled connection.
const health=()=>new Promise((resolve,reject)=>{
 const request=http.get(base+'/api/health',{agent:false},response=>{
  let body='';response.setEncoding('utf8');response.on('data',chunk=>body+=chunk);
  response.on('error',reject);response.on('end',()=>{try{assert.equal(response.statusCode,200);resolve(JSON.parse(body))}catch(error){reject(error)}});
 });
 request.on('error',reject);request.setTimeout(5000,()=>request.destroy(Error('Health probe timed out')));
});
const checks=[];
try {
 assert.equal(run(process.execPath,[install,'--offline'],false),null);checks.push('missing offline artifact fails without fallback');
 const rows=[{type:'session_meta',payload:{id:'native-only-fixture',model:'gpt-5',cwd:path.join(tmp,'project')}},{timestamp:'2026-10-01T00:00:00Z',type:'event_msg',payload:{type:'token_count',info:{total_token_usage:{input_tokens:10,output_tokens:2,total_tokens:12}}}}];
 await writeFile(path.join(codex,'sessions/rollout-native.jsonl'),rows.map(r=>JSON.stringify(r)).join('\n')+'\n');
 // A manual download must work with its original asset filename and no skill.
 const direct=path.join(tmp,entry.file);
 await copyFile(path.join(root,'dist/release',entry.file),direct);await chmod(direct,0o755);
 run(direct,launchArgs);await until(async()=> (await health()).index.phase==='ready');
 assert.equal((await health()).build_id,entry.build_id);
 assert.equal(JSON.parse(run(direct,['--once','--json','--codex-home',codex,'--no-auto-windows'])).summary.usage.total_tokens,12);
 run(direct,['--stop']);assert(!await occupied());
 checks.push('direct release download starts, reads logs and stops without a skill or interpreters');
 run(process.execPath,installArgs);assert.equal((await readFile(path.join(skill,'SKILL.md'),'utf8')).includes('native executable'),true);
 assert.deepEqual(await readFile(binary),await readFile(direct));
 assert(!(await readdir(path.join(skill,'scripts'))).some(name=>name.endsWith('.py')));checks.push('fresh offline skill install uses exactly the same executable as the direct download');
 run(binary,launchArgs);await until(async()=> (await health()).index.phase==='ready');
 let h=await health();assert.equal(h.build_id,entry.build_id);assert.equal(h.implementation,'rust');assert.equal(h.ui,'svelte');
 const page=await(await fetch(base,{headers:{Connection:'close'}})).text();
 assert.equal(page,await readFile(path.join(root,'web/dist/index.html'),'utf8'));
 assert.equal(await(await fetch(base+'/index.html')).text(),page);
 for(const asset of [...page.matchAll(/(?:src|href)="(\/assets\/[^\"]+)"/g)])assert.equal((await fetch(base+asset[1])).status,200);
 assert.equal((await fetch(base+'/legacy')).status,404);
 for(const route of ['/api/sessions','/api/session','/snapshot-export.js'])assert.equal((await fetch(base+route)).status,404);
 for(const old of ['/svelte','/svelte/']) {
   const redirected=await fetch(base+old+'?page=settings&timezone=Asia%2FShanghai',{redirect:'manual'});
   assert.equal(redirected.status,308);assert.equal(redirected.headers.get('location'),'/?page=settings&timezone=Asia%2FShanghai');
 }
 checks.push('root and index serve current UI and assets; classic routes removed; old bookmarks redirect');
 const ensure=path.join(skill,'scripts/install_runtime.mjs');
 assert.equal(run(process.execPath,[ensure,'--ensure']).trim(),binary);
 assert.equal((await health()).pid,h.pid);checks.push('skill readiness check reuses matching runtime offline');
 const skillAlias=path.join(tmp,'linked-skill');
 await symlink(skill,skillAlias,process.platform==='win32'?'junction':'dir');
 assert.equal(run(process.execPath,[path.join(skillAlias,'scripts/install_runtime.mjs'),'--ensure']).trim(),binary);
 assert.equal((await health()).pid,h.pid);checks.push('skill invoked through a symlink resolves and verifies its runtime');
 const pid=h.pid;run(binary,launchArgs);assert.equal((await health()).pid,pid);checks.push('same build and source scope reuse');
 // Two concurrent invocations must still resolve to one server.
 await Promise.all([1,2].map(()=>new Promise((resolve,reject)=>{const p=spawn(binary,launchArgs,{env,cwd:tmp,stdio:'ignore'});p.on('error',reject);p.on('exit',code=>code===0?resolve():reject(Error(`launcher exit ${code}`)))})));
 assert.equal((await health()).pid,pid);checks.push('concurrent launches serialize');
 const once=JSON.parse(run(binary,['--once','--json','--codex-home',codex,'--no-auto-windows']));assert.equal(once.summary.usage.total_tokens,12);
 run(binary,['--export-snapshot','relative-export.json','--codex-home',codex,'--no-auto-windows']);
 const exported=JSON.parse(await readFile(path.join(tmp,'relative-export.json'),'utf8'));assert.equal(exported.version,1);assert.equal(exported.snapshot.summary.usage.total_tokens,12);assert.equal((await health()).pid,pid);checks.push('standalone export and once coexist with server without Python');
 run(binary,['--export-snapshot','--codex-home',codex,'--no-auto-windows']);
 const gzipPath=(await readdir(tmp)).find(name=>/^cousash-.*\.json\.gz$/.test(name));assert(gzipPath);
 const compressedExport=JSON.parse(gunzipSync(await readFile(path.join(tmp,gzipPath))).toString('utf8'));
 assert.equal(compressedExport.snapshot.summary.usage.total_tokens,12);assert.deepEqual(compressedExport.snapshot.details_by_uid,exported.snapshot.details_by_uid);
 const download=await fetch(base+'/api/export.json.gz');assert.equal(download.headers.get('content-type'),'application/gzip');assert(download.headers.get('content-disposition').includes('.json.gz'));
 const httpExport=JSON.parse(gunzipSync(Buffer.from(await download.arrayBuffer())).toString('utf8'));assert.deepEqual(httpExport.snapshot.details_by_uid,exported.snapshot.details_by_uid);checks.push('default CLI and HTTP gzip exports retain complete session details');
 const bad=path.join(tmp,'bad');await mkdir(bad);const badManifest=structuredClone(manifest);badManifest.platforms[platformKey()].sha256='0'.repeat(64);
 await writeFile(path.join(bad,'manifest.json'),JSON.stringify(badManifest));await copyFile(path.join(root,'dist/release',entry.file),path.join(bad,entry.file));
 const before=await readFile(path.join(runtime,'manifest.json'),'utf8');
 assert.equal(run(process.execPath,[install,'--runtime-manifest',path.join(bad,'manifest.json'),'--offline'],false),null);
 assert.equal(await readFile(path.join(runtime,'manifest.json'),'utf8'),before);assert.equal((await health()).pid,pid);checks.push('checksum failure preserves install and running process');
 // An upgrade preserves obsolete files that may contain user changes.
 await writeFile(path.join(skill,'scripts/open_dashboard.py'),'obsolete');await mkdir(path.join(config,'remotes'),{recursive:true});await writeFile(path.join(config,'remotes/preserve.txt'),'sentinel');
 run(process.execPath,[...installArgs,'--force']);assert.equal(await readFile(path.join(skill,'scripts/open_dashboard.py'),'utf8'),'obsolete');assert.equal(await readFile(path.join(config,'remotes/preserve.txt'),'utf8'),'sentinel');checks.push('upgrade preserves obsolete files and external user data');
 // Skills CLI updates can leave a previous runtime behind. Mock release transport
 // only; exercise the real ensure, verification, replacement and cleanup paths.
 run(binary,launchArgs);
 const stale=structuredClone(manifest);stale.version='0.2.0';
 await writeFile(path.join(runtime,'manifest.json'),JSON.stringify(stale));
 await writeFile(path.join(skill,'scripts/open_dashboard.py'),'obsolete');
 const transport=path.join(tmp,'release-transport.mjs');
 await writeFile(transport,`import {readFile} from 'node:fs/promises';\nconst manifest=${JSON.stringify(manifest)};\nconst artifact=${JSON.stringify(path.join(root,'dist/release',entry.file))};\nglobalThis.fetch=async url=>{if(!String(url).includes('/v'+manifest.version+'/'))throw Error('Wrong release requested');return {ok:true,url:String(url),body:[String(url).endsWith('/manifest.json')?Buffer.from(JSON.stringify(manifest)):await readFile(artifact)]}};\n`);
 const skillOnly=path.join(tmp,'skill-only');await mkdir(path.join(skillOnly,'scripts'),{recursive:true});
 for(const file of ['SKILL.md','native.json','scripts/install_runtime.mjs'])await copyFile(path.join(root,'skills/codex-usage-dashboard',file),path.join(skillOnly,file));
 const freshBinary=path.join(skillOnly,'runtime',binaryName());
 assert.equal(run(process.execPath,['--import',pathToFileURL(transport).href,path.join(skillOnly,'scripts/install_runtime.mjs'),'--ensure']).trim(),freshBinary);
 assert.deepEqual(await readFile(freshBinary),await readFile(direct));
 checks.push('fresh skill-only installation downloads and verifies the same release executable (mock HTTPS transport)');
 assert.equal(run(process.execPath,['--import',pathToFileURL(transport).href,ensure,'--ensure']).trim(),binary);
 assert.equal(JSON.parse(await readFile(path.join(runtime,'manifest.json'),'utf8')).version,manifest.version);
 assert.equal(await readFile(path.join(skill,'scripts/open_dashboard.py'),'utf8'),'obsolete');
 run(binary,launchArgs);assert.equal((await health()).ui,'svelte');
 checks.push('skills CLI update replaces stale runtime with the skill release and preserves obsolete files');
 run(process.execPath,installArgs);run(binary,launchArgs);run(process.execPath,[path.join(skill,'scripts/install_runtime.mjs'),'--rollback']);run(binary,launchArgs);checks.push('verified native-only rollback');
 // Explicit source changes must replace the service even for the same build.
 const oldPid=(await health()).pid;const empty=path.join(tmp,'empty');await mkdir(empty);
 run(binary,['--no-browser','--codex-home',empty,'--no-auto-windows']);assert.notEqual((await health()).pid,oldPid);assert.deepEqual((await health()).codex_homes,[process.platform==='win32'?path.toNamespacedPath(empty):empty]);checks.push('source changes replace managed process');
 if(process.platform==='win32') {
  run(binary,['--stop']);
  const powershell=path.join(process.env.SystemRoot,'System32/WindowsPowerShell/v1.0/powershell.exe');
  const holder=spawn(powershell,['-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',path.join(root,'tools/fixtures/windows_port_hold.ps1')],{env,stdio:['pipe','pipe','pipe'],windowsHide:true});
  let output='',errors='';holder.stdout.on('data',data=>output+=data);holder.stderr.on('data',data=>errors+=data);
  const holderDone=new Promise((resolve,reject)=>{holder.once('error',reject);holder.once('exit',code=>code===0?resolve():reject(Error(`Port holder failed: ${errors}`)))});
  holderDone.catch(()=>{});
  try {
   await until(async()=>{if(holder.exitCode!==null)throw Error(errors);return output.includes('holding')});
   assert(!await occupied(),'bound socket must be invisible to connection probes');
   const probe=net.createServer();
   const bindError=await new Promise(resolve=>{probe.once('error',resolve);probe.listen(8765,'127.0.0.1',()=>probe.close(()=>resolve(null)))});
   assert.equal(bindError?.code,'EADDRINUSE','fixture must retain the real Windows binding');
   const launch=spawn(binary,launchArgs,{env,cwd:tmp,stdio:'ignore',windowsHide:true});
   const launchDone=new Promise((resolve,reject)=>{launch.once('error',reject);launch.once('exit',code=>code===0?resolve():reject(Error(`Delayed launcher exit ${code}`)))});
   launchDone.catch(()=>{});
   await until(async()=>{
    if(launch.exitCode!==null)throw Error(`Delayed launcher exit ${launch.exitCode}`);
    return (await readFile(path.join(config,'dashboard.log'),'utf8')).includes('Waiting for Windows to release port 8765');
   });
   assert.equal(launch.exitCode,null,'launcher must wait for release instead of failing');
   holder.stdin.end('\n');await holderDone;await launchDone;
   assert.equal((await health()).build_id,entry.build_id);
   assert((await readFile(path.join(config,'dashboard.log'),'utf8')).includes('Waiting for Windows to release port 8765'));
   checks.push('Windows waits for an invisible previous binding, then launches on the same port');
  } finally {
   if(holder.exitCode===null)holder.stdin.end('\n');
   await holderDone;
  }
 }
 const stream=await fetch(base+'/api/revisions');const reader=stream.body.getReader();await reader.read();run(binary,['--stop']);await reader.cancel();assert(!await occupied());checks.push('authenticated stop closes SSE and releases writer/port');
 // Child process invocation cannot block this Node HTTP handler: run async here.
 const foreign=http.createServer((req,res)=>{res.end(JSON.stringify({app:'unrelated'}))});await new Promise(r=>foreign.listen(8765,'127.0.0.1',r));
 try {const code=await new Promise((resolve,reject)=>{const child=spawn(binary,launchArgs,{env,cwd:tmp,stdio:'ignore'});child.on('error',reject);child.on('exit',resolve)});assert.notEqual(code,0);assert(await occupied())}
 finally {await new Promise(r=>foreign.close(r))}
 checks.push('foreign port owner remains running');
 const report={passed:true,platform:platformKey(),version:manifest.version,ui:manifest.ui,build_id:entry.build_id,python_on_path:false,checks};
 await mkdir(path.join(root,'benchmarks/implementation/results'),{recursive:true});await writeFile(path.join(root,'benchmarks/implementation/results',`native-only-${platformKey()}.json`),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report));
} catch(error) { console.error('Completed checks:',checks); console.error(await readFile(path.join(config,'dashboard.log'),'utf8').catch(()=>'')); throw error; } finally {
 if(await occupied()) {try{run(binary,['--stop'])}catch(e){console.error(e.message)}}
 await rm(tmp,{recursive:true,force:true});
 if(replace) execFileSync(worktree,['--no-browser'],{stdio:'inherit'});
}
