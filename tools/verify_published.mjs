// Verify the public release, without credentials, on the runner's native platform.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {mkdtemp,realpath,mkdir,readFile,writeFile,chmod,rm} from 'node:fs/promises';
import http from 'node:http';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {platformKey,binaryName} from '../skills/codex-usage-dashboard/scripts/install_runtime.mjs';
import {checkRelease,platforms} from './check_release.mjs';
import {nativeAssetName} from './release-names.mjs';

const root=path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const config=await checkRelease(process.env.RELEASE_TAG);
const key=platformKey();
const release=`https://github.com/${config.repository}/releases/download/v${config.version}/`;
const occupied=()=>new Promise(resolve=>{const socket=net.connect(8765,'127.0.0.1');socket.once('connect',()=>{socket.destroy();resolve(true)});socket.once('error',()=>resolve(false))});
const worktree=path.join(root,'target/release',binaryName());
const replace=process.argv.includes('--replace-worktree');
if(replace) execFileSync(worktree,['--stop'],{windowsHide:true});
assert(!await occupied(),'8765 must be free; unrelated processes must not be stopped');
const tmp=await realpath(await mkdtemp(path.join(os.tmpdir(),'published-dashboard-')));
const codex=path.join(tmp,'codex');
const noPath=path.join(tmp,'no-interpreters');
await mkdir(path.join(codex,'sessions'),{recursive:true});await mkdir(noPath);
const env={...process.env,PATH:noPath,CODEX_HOME:codex,COUSASH_CONFIG_DIR:path.join(tmp,'config'),TZ:'UTC'};
const run=(file,args)=>execFileSync(file,args,{env,cwd:tmp,encoding:'utf8',timeout:120000,windowsHide:true});
const health=()=>new Promise((resolve,reject)=>{
 const request=http.get('http://127.0.0.1:8765/api/health',{agent:false},response=>{
  let body='';response.setEncoding('utf8');response.on('data',part=>body+=part);
  response.on('error',reject);response.on('end',()=>{try{assert.equal(response.statusCode,200);resolve(JSON.parse(body))}catch(error){reject(error)}});
 });request.on('error',reject);request.setTimeout(5000,()=>request.destroy(Error('Health timeout')));
});
const download=async name=>{
 const response=await fetch(release+name,{signal:AbortSignal.timeout(120000)});
 assert.equal(response.status,200,`Public download failed: ${name}`);
 return Buffer.from(await response.arrayBuffer());
};
const checks=[];let active;
try {
 const manifest=JSON.parse(await download('manifest.json'));
 assert.equal(manifest.version,config.version);assert.equal(manifest.protocol_version,1);assert.equal(manifest.ui,'svelte');
 assert.deepEqual(Object.keys(manifest.platforms).sort(),[...platforms].sort());
 const entry=manifest.platforms[key];
 assert.match(entry.build_id,/^[a-f0-9]{64}$/);assert.match(entry.sha256,/^[a-f0-9]{64}$/);
 assert.equal(entry.file,nativeAssetName(config.version,key));
 const bytes=await download(entry.file);
 assert.equal(createHash('sha256').update(bytes).digest('hex'),entry.sha256);
 const sums=(await download('SHA256SUMS')).toString('utf8');
 assert(sums.split('\n').includes(`${entry.sha256}  ${entry.file}`));
 checks.push('public manifest, executable and SHA256SUMS agree');
 const direct=path.join(tmp,entry.file);await writeFile(direct,bytes);await chmod(direct,0o755);
 assert.deepEqual(JSON.parse(run(direct,['--build-info'])),{version:config.version,ui:'svelte',build_id:entry.build_id});
 const rows=[{type:'session_meta',payload:{id:'published-fixture',model:'gpt-5',cwd:path.join(tmp,'project')}},{timestamp:'2026-10-01T00:00:00Z',type:'event_msg',payload:{type:'token_count',info:{total_token_usage:{input_tokens:10,output_tokens:2,total_tokens:12}}}}];
 await writeFile(path.join(codex,'sessions/rollout-published.jsonl'),rows.map(row=>JSON.stringify(row)).join('\n')+'\n');
 const smoke=async binary=>{
  active=binary;run(binary,['--no-browser','--codex-home',codex,'--no-auto-windows','--timezone','UTC']);
  const deadline=Date.now()+60000;let h;
  do {h=await health();if(h.index.phase==='ready')break;assert(Date.now()<deadline,'Index did not become ready');await new Promise(resolve=>setTimeout(resolve,100))} while(true);
  assert.equal(h.version,config.version);assert.equal(h.build_id,entry.build_id);assert.equal(h.ui,'svelte');
  const page=await fetch('http://127.0.0.1:8765/',{headers:{Connection:'close'}});assert.equal(page.status,200);assert((await page.text()).includes('/assets/'));
  const usage=JSON.parse(run(binary,['--once','--json','--codex-home',codex,'--no-auto-windows']));assert.equal(usage.summary.usage.total_tokens,12);
  run(binary,['--stop']);assert(!await occupied());active=undefined;
 };
 await smoke(direct);checks.push('direct public download starts, serves UI, reads usage and stops without interpreters');
 run(process.execPath,[path.join(root,'bin/install.js')]);
 const skill=path.join(codex,'skills/codex-usage-dashboard');
 const binary=path.join(skill,'runtime',binaryName());
 assert((await readFile(path.join(skill,'SKILL.md'),'utf8')).includes('native executable'));
 assert.deepEqual(await readFile(binary),bytes);checks.push('fresh skill installation downloads the exact public executable');
 const ensure=path.join(skill,'scripts/install_runtime.mjs');
 assert.equal(run(process.execPath,[ensure,'--ensure']).trim(),binary);
 await smoke(binary);checks.push('installed skill verifies and runs the published runtime');
 const stale=structuredClone(manifest);stale.version='0.0.0';
 await writeFile(path.join(skill,'runtime/manifest.json'),JSON.stringify(stale));
 assert.equal(run(process.execPath,[ensure,'--ensure']).trim(),binary);
 assert.equal(JSON.parse(await readFile(path.join(skill,'runtime/manifest.json'),'utf8')).version,config.version);
 assert.deepEqual(await readFile(binary),bytes);await smoke(binary);
 checks.push('outdated skill runtime metadata triggers a successful public release upgrade');
 const report={passed:true,version:config.version,platform:key,release,build_id:entry.build_id,sha256:entry.sha256,checks};
 await mkdir(path.join(root,'output'),{recursive:true});
 await writeFile(path.join(root,`output/published-${key}.json`),JSON.stringify(report,null,2)+'\n');
 console.log(JSON.stringify(report));
} finally {
 if(active) {try{run(active,['--stop'])}catch(error){console.error(error.message)}}
 await rm(tmp,{recursive:true,force:true});
 if(replace) execFileSync(worktree,['--no-browser'],{stdio:'inherit',windowsHide:true});
}
