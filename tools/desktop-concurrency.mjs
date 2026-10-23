// Race cold desktop launches and verify isolation between live configurations.
import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {mkdtemp,mkdir,readFile,writeFile,open,rm} from 'node:fs/promises';
import {once} from 'node:events';
import path from 'node:path';
import net from 'node:net';
import {fileURLToPath} from 'node:url';
import {runtimeDir,cacheDir} from './runtime-paths.mjs';
const root=fileURLToPath(new URL('../',import.meta.url)),target=spawnSync('rustc',['-vV'],{encoding:'utf8'}).stdout.match(/host: (.+)/)[1];
const output=path.join(path.resolve(root,'desktop',process.env.COUSASH_DESKTOP_TARGET_DIR||'target'),target,'release');
const host=process.platform==='darwin'?path.join(output,'bundle/macos/Codex Usage Dashboard.app/Contents/MacOS/cousash-desktop'):path.join(output,'cousash-desktop'+(process.platform==='win32'?'.exe':''));
const helper=path.join(path.dirname(host),'codex-usage-dashboard'+(process.platform==='win32'?'.exe':''));
const info=JSON.parse(spawnSync(helper,['--build-info'],{encoding:'utf8'}).stdout);
const parent=path.join(root,'output');await mkdir(parent,{recursive:true});const dir=await mkdtemp(path.join(parent,'desktop-race-'));
const states=[path.join(dir,'first'),path.join(dir,'second')];for(const state of states)await mkdir(state);
const children=[],logs=[],exits=new Map();const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function launch(state){const log=await open(path.join(dir,'host-'+children.length+'.log'),'w');logs.push(log);const child=spawn(host,[],{cwd:dir,env:{...process.env,COUSASH_CONFIG_DIR:state,CODEX_HOME:path.join(root,'output/gui-fixture')},windowsHide:true,stdio:['ignore',log.fd,log.fd]});children.push(child);exits.set(child,once(child,'exit'));return child;}
async function deadline(promise,ms){let timer;try{return await Promise.race([promise,new Promise((_,reject)=>timer=setTimeout(()=>reject(Error('Desktop concurrency timeout')),ms))]);}finally{clearTimeout(timer);}}
async function metadata(state,name){return JSON.parse(await readFile(path.join(runtimeDir(state),name),'utf8'));}
async function ready(state){const end=Date.now()+90000;while(Date.now()<end){try{const desktop=await metadata(state,'desktop.json'),record=await metadata(state,'instance.json');assert(children.some(child=>child.pid===desktop.pid));const health=await(await fetch('http://'+record.address+'/api/health',{signal:AbortSignal.timeout(1000)})).json();assert.equal(health.build_id,info.build_id);if(health.index.phase==='ready')return{desktop,record,base:'http://'+record.address};}catch{}await pause(50);}throw Error('Desktop readiness timeout');}
async function quit(state){const value=await metadata(state,'desktop.json');await new Promise((resolve,reject)=>{const socket=net.connect(Number(value.address.split(':')[1]),'127.0.0.1');socket.setTimeout(10000,()=>socket.destroy(Error('Quit timeout')));socket.on('connect',()=>socket.write(JSON.stringify({token:value.token,action:'quit'})+'\n'));socket.on('data',()=>{});socket.on('end',resolve);socket.on('error',reject);});}
try{
  const first=await launch(states[0]),second=await launch(states[0]);const a=await ready(states[0]);
  const winner=[first,second].find(child=>child.pid===a.desktop.pid),loser=[first,second].find(child=>child!==winner);assert(winner&&loser);
  await deadline(exits.get(loser),10000);assert.equal(loser.exitCode,0);assert.equal(winner.exitCode,null);
  const loserLog=await readFile(path.join(dir,'host-'+children.indexOf(loser)+'.log'),'utf8');assert.doesNotMatch(loserLog,/desktop\.lock is busy/);
  const isolated=await launch(states[1]),b=await ready(states[1]);assert.equal(isolated.pid,b.desktop.pid);assert.notEqual(a.record.address,b.record.address);assert.notEqual(a.record.pid,b.record.pid);
  await quit(states[1]);await deadline(exits.get(isolated),10000);assert.equal(isolated.exitCode,0);
  assert.equal((await(await fetch(a.base+'/api/health')).json()).pid,a.record.pid);assert.equal(winner.exitCode,null);
  await quit(states[0]);await deadline(exits.get(winner),10000);assert.equal(winner.exitCode,0);
  const saved=JSON.parse(await readFile(path.join(states[0],'desktop-settings.json'),'utf8'));
  const reopened=await launch(states[0]),c=await ready(states[0]);assert.equal(c.desktop.pid,reopened.pid);
  await quit(states[0]);await deadline(exits.get(reopened),10000);assert.equal(reopened.exitCode,0);
  const restored=JSON.parse(await readFile(path.join(states[0],'desktop-settings.json'),'utf8'));
  assert(Math.abs(saved.width-restored.width)<=1,`Window width drift: ${saved.width} -> ${restored.width}`);
  assert(Math.abs(saved.height-restored.height)<=1,`Window height drift: ${saved.height} -> ${restored.height}`);
  const report={platform:process.platform,target,build_id:info.build_id,concurrent_cold_launches:2,one_host:true,loser_exit:0,independent_live_configurations:true,other_configuration_preserved_on_quit:true,window_state_roundtrip:true,window_size:{width:restored.width,height:restored.height}};
  await mkdir(path.join(root,'output/verification'),{recursive:true});await writeFile(path.join(root,'output/verification',`desktop-concurrency-${process.platform}.json`),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report));
}catch(error){for(let i=0;i<children.length;i++)console.error((await readFile(path.join(dir,'host-'+i+'.log'),'utf8')).slice(-3000));throw error;
}finally{
  for(const state of states)try{await quit(state);}catch{}
  for(const child of children){if(child.exitCode===null){child.kill();await deadline(exits.get(child),10000);} }
  for(const log of logs)await log.close();
  // Only remove these freshly generated, task-owned fixtures.
  if(process.platform!=='win32')for(const state of states){assert.match(path.basename(cacheDir(state)),/^[a-f0-9]{64}$/);await rm(cacheDir(state),{recursive:true,force:true});}
  assert(dir.startsWith(parent+path.sep)&&path.basename(dir).startsWith('desktop-race-'));await rm(dir,{recursive:true,force:true,maxRetries:20,retryDelay:100});
}
