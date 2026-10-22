#!/usr/bin/env node
// Fresh installation and real native lifecycle, on 8765, with no Python on PATH.
import assert from 'node:assert/strict';
import {execFileSync,spawn} from 'node:child_process';
import {mkdtemp,mkdir,readFile,writeFile,rm,copyFile,readdir} from 'node:fs/promises';
import http from 'node:http';
import net from 'node:net';
import path from 'node:path';
import os from 'node:os';
import {gunzipSync} from 'node:zlib';
import {fileURLToPath} from 'node:url';
import {binaryName,platformKey} from '../skills/codex-usage-dashboard/scripts/install_runtime.mjs';
const root=path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const worktree=path.join(root,'target/release',binaryName());
const base='http://127.0.0.1:8765';
const replace=process.argv.includes('--replace-worktree');
const occupied=()=>new Promise(resolve=>{const socket=net.connect(8765,'127.0.0.1');socket.once('connect',()=>{socket.destroy();resolve(true)});socket.once('error',()=>resolve(false))});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const until=async test=>{const end=Date.now()+60000;while(!await test()){if(Date.now()>end)throw Error('Timed out');await sleep(50)}};
if(replace) execFileSync(worktree,['--stop']);
assert(!await occupied(),'8765 must be free; existing process was not stopped');
const tmp=await mkdtemp(path.join(os.tmpdir(),'native-only-'));
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
const health=async()=>{const r=await fetch(base+'/api/health',{headers:{Connection:'close'}});assert.equal(r.status,200);return r.json()};
const checks=[];
try {
 assert.equal(run(process.execPath,[install,'--offline'],false),null);checks.push('missing offline artifact fails without fallback');
 run(process.execPath,installArgs);assert.equal((await readFile(path.join(skill,'SKILL.md'),'utf8')).includes('native executable'),true);
 assert(!(await readdir(path.join(skill,'scripts'))).some(name=>name.endsWith('.py')));checks.push('fresh offline install ships no Python backend or launcher');
 const rows=[{type:'session_meta',payload:{id:'native-only-fixture',model:'gpt-5',cwd:path.join(tmp,'project')}},{timestamp:'2026-10-01T00:00:00Z',type:'event_msg',payload:{type:'token_count',info:{total_token_usage:{input_tokens:10,output_tokens:2,total_tokens:12}}}}];
 await writeFile(path.join(codex,'sessions/rollout-native.jsonl'),rows.map(r=>JSON.stringify(r)).join('\n')+'\n');
 run(binary,launchArgs);await until(async()=> (await health()).index.phase==='ready');
 let h=await health();assert.equal(h.build_id,entry.build_id);assert.equal(h.implementation,'rust');assert.equal(h.ui,'legacy');
 assert.equal(await(await fetch(base,{headers:{Connection:'close'}})).text(),await readFile(path.join(root,'web/classic/index.html'),'utf8'));checks.push('native launch with empty PATH, correct build and original UI');
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
 // Legacy installed files disappear only after a successful native upgrade.
 await writeFile(path.join(skill,'scripts/open_dashboard.py'),'obsolete');await mkdir(path.join(config,'remotes'),{recursive:true});await writeFile(path.join(config,'remotes/preserve.txt'),'sentinel');
 run(process.execPath,[...installArgs,'--force']);assert(!(await readdir(path.join(skill,'scripts'))).includes('open_dashboard.py'));assert.equal(await readFile(path.join(config,'remotes/preserve.txt'),'utf8'),'sentinel');checks.push('upgrade removes old scripts and preserves external user data');
 run(binary,launchArgs);run(process.execPath,[path.join(skill,'scripts/install_runtime.mjs'),'--rollback']);run(binary,launchArgs);checks.push('verified native-only rollback');
 // Explicit source changes must replace the service even for the same build.
 const oldPid=(await health()).pid;const empty=path.join(tmp,'empty');await mkdir(empty);
 run(binary,['--no-browser','--codex-home',empty,'--no-auto-windows']);assert.notEqual((await health()).pid,oldPid);assert.deepEqual((await health()).codex_homes,[process.platform==='win32'?path.toNamespacedPath(empty):empty]);checks.push('source changes replace managed process');
 const stream=await fetch(base+'/api/revisions');const reader=stream.body.getReader();await reader.read();run(binary,['--stop']);await reader.cancel();assert(!await occupied());checks.push('authenticated stop closes SSE and releases writer/port');
 // Child process invocation cannot block this Node HTTP handler: run async here.
 const foreign=http.createServer((req,res)=>{res.end(JSON.stringify({app:'unrelated'}))});await new Promise(r=>foreign.listen(8765,'127.0.0.1',r));
 try {const code=await new Promise((resolve,reject)=>{const child=spawn(binary,launchArgs,{env,cwd:tmp,stdio:'ignore'});child.on('error',reject);child.on('exit',resolve)});assert.notEqual(code,0);assert(await occupied())}
 finally {await new Promise(r=>foreign.close(r))}
 checks.push('foreign port owner remains running');
 const report={passed:true,platform:platformKey(),build_id:entry.build_id,python_on_path:false,checks};
 await mkdir(path.join(root,'benchmarks/implementation/results'),{recursive:true});await writeFile(path.join(root,'benchmarks/implementation/results',`native-only-${platformKey()}.json`),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report));
} catch(error) { console.error('Completed checks:',checks); console.error(await readFile(path.join(config,'dashboard.log'),'utf8').catch(()=>'')); throw error; } finally {
 if(await occupied()) {try{run(binary,['--stop'])}catch(e){console.error(e.message)}}
 await rm(tmp,{recursive:true,force:true});
 if(replace) execFileSync(worktree,['--no-browser'],{stdio:'inherit'});
}
