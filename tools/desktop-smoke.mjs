// Native host + sidecar smoke test. It does not claim GUI interaction coverage.
import assert from 'node:assert/strict';
import {runtimeDir,cacheDir} from './runtime-paths.mjs';
import {spawn,spawnSync} from 'node:child_process';
import {mkdir,mkdtemp,writeFile,readFile,rm,open,stat} from 'node:fs/promises';
import {once} from 'node:events';
import net from 'node:net';
import path from 'node:path';
import os from 'node:os';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const target=process.env.DESKTOP_TARGET||spawnSync('rustc',['-vV'],{encoding:'utf8'}).stdout.match(/host: (.+)/)[1];
const output=path.join(path.resolve(root,'desktop',process.env.COUSASH_DESKTOP_TARGET_DIR||'target'),target,'release');
const host=process.env.SMOKE_DESKTOP_BINARY|| (process.platform==='darwin'?path.join(output,'bundle/macos/Codex Usage Dashboard.app/Contents/MacOS/cousash-desktop'):path.join(output,'cousash-desktop'+(process.platform==='win32'?'.exe':'')));
const service=process.env.SMOKE_SERVICE_BINARY||path.join(path.dirname(host),'codex-usage-dashboard'+(process.platform==='win32'?'.exe':''));
const hostArgs=JSON.parse(process.env.SMOKE_DESKTOP_ARGS||'[]');assert(Array.isArray(hostArgs)&&hostArgs.every(value=>typeof value==='string'));
const info=JSON.parse(spawnSync(service,['--build-info'],{encoding:'utf8'}).stdout);
const parent=path.resolve(process.env.SMOKE_WORK_DIR||path.join(root,'output'));await mkdir(parent,{recursive:true});
const dir=await mkdtemp(path.join(parent,'desktop-smoke-'));assert(dir.startsWith(parent));
const home=path.join(dir,'Codex 日志');const state=path.join(dir,'state');await mkdir(path.join(home,'sessions'),{recursive:true});
const sessionCount=Number(process.env.SMOKE_SESSIONS||1),eventCount=Number(process.env.SMOKE_EVENTS||1);
assert(Number.isInteger(sessionCount)&&sessionCount>0&&sessionCount<=10000);assert(Number.isInteger(eventCount)&&eventCount>0&&eventCount<=10000);
for(let session=0;session<sessionCount;session++){
  const rows=[{type:'session_meta',payload:{id:'desktop-smoke-'+session,model:'gpt-5',cwd:'/synthetic/desktop'}},{timestamp:'2026-10-23T00:00:00Z',type:'event_msg',payload:{type:'user_message',message:'Desktop smoke fixture'}}];
  for(let event=1;event<=eventCount;event++){const input=Math.round(10000*event/eventCount),output=Math.round(2000*event/eventCount);rows.push({timestamp:new Date(Date.UTC(2026,9,23,0,0,event)).toISOString(),type:'event_msg',payload:{type:'token_count',info:{total_token_usage:{input_tokens:input,cached_input_tokens:Math.round(8000*event/eventCount),output_tokens:output,total_tokens:input+output}}}});}
  await writeFile(path.join(home,`sessions/fixture-${session}.jsonl`),rows.map(JSON.stringify).join('\n')+'\n');
}
await mkdir(state,{recursive:true});const runtime=runtimeDir(state);
const env={...process.env,COUSASH_CONFIG_DIR:state,CODEX_HOME:home};
const log=await open(path.join(dir,'host.log'),'w');
const started=performance.now();const child=spawn(host,hostArgs,{cwd:dir,env,windowsHide:true,stdio:['ignore',log.fd,log.fd]});const exited=once(child,'exit');
const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function withDeadline(promise,ms,message){let timer;try{return await Promise.race([promise,new Promise((_,reject)=>timer=setTimeout(()=>reject(Error(message)),ms))]);}finally{clearTimeout(timer);}}
const read=async name=>JSON.parse(await readFile(path.join(runtime,name),'utf8'));
let record,desktop,startupMs;
async function activate(action){
  desktop=await read('desktop.json');if(process.env.SMOKE_ALLOW_WRAPPER!=='1')assert.equal(desktop.pid,child.pid);
  const[address,port]=desktop.address.split(':');assert.equal(address,'127.0.0.1');
  await new Promise((resolve,reject)=>{const socket=net.connect(Number(port),address);socket.setTimeout(3000,()=>socket.destroy(Error('Activation timeout')));let reply='';socket.on('connect',()=>socket.write(JSON.stringify({token:desktop.token,action})+'\n'));socket.on('data',data=>reply+=data);socket.on('error',reject);socket.on('end',()=>reply.trim()==='ok'?resolve():reject(Error('Activation refused')));});
}
try{
  while(performance.now()-started<90000){
    if(child.exitCode!==null)throw Error('Host exited: '+await readFile(path.join(dir,'host.log'),'utf8'));
    try{
      record=await read('instance.json');const health=await(await fetch('http://'+record.address+'/api/health',{signal:AbortSignal.timeout(2000)})).json();
      assert.equal(health.instance_id,record.instance_id);assert.equal(health.build_id,info.build_id);
      if(health.index.phase==='error')throw Error('Index failed: '+health.index.error);
      if(health.index.phase==='ready'){startupMs=Math.round(performance.now()-started);break;}
    }catch(error){if(error.code!=='ENOENT'&&!/fetch failed|timeout/i.test(String(error)))throw error;}
    await pause(100);
  }
  assert(startupMs!==undefined,'Service readiness timeout');
  const base='http://'+record.address;
  const timings=[];let summary;for(let i=0;i<5;i++){const start=performance.now();summary=await(await fetch(base+'/api/summary?period=all')).json();timings.push(performance.now()-start);}assert.equal(summary.summary.usage.total_tokens,12000*sessionCount);
  await activate('show');
  const duplicate=spawn(host,hostArgs,{cwd:dir,env,windowsHide:true,stdio:'ignore'});try{await withDeadline(once(duplicate,'exit'),10000,'Second launch did not return');}catch(error){duplicate.kill();throw error;}
  assert.equal(duplicate.exitCode,0);assert.equal((await read('desktop.json')).pid,desktop.pid);assert.equal((await read('instance.json')).pid,record.pid);
  const report={kind:'native-host-and-service-smoke',platform:process.platform,arch:process.arch,os:os.release(),target,build_id:info.build_id,startup_ms:startupMs,sessions:sessionCount,token_events:sessionCount*eventCount,summary_median_ms:Math.round(timings.sort((a,b)=>a-b)[2]*10)/10,tokens:summary.summary.usage.total_tokens,single_instance:true};
  if(process.platform!=='win32'){assert.equal((await stat(runtime)).mode&0o077,0);assert.equal((await stat(path.join(runtime,'instance.json'))).mode&0o077,0);report.private_runtime_verified=true;}
  await mkdir(path.join(root,'output/verification'),{recursive:true});
  await writeFile(path.join(root,'output/verification',`desktop-${sessionCount>1?'performance':'smoke'}-${process.platform}-${process.arch}.json`),JSON.stringify(report,null,2)+'\n');
  console.log(JSON.stringify(report));
}finally{
  try{await activate('quit');}catch{}
  try{await withDeadline(exited,10000,'Host exit timed out');}catch{child.kill();await exited;}
  if(startupMs!==undefined&&child.exitCode===0){const prefs=JSON.parse(await readFile(path.join(state,'desktop-settings.json'),'utf8'));assert(prefs.width>0&&prefs.height>0,'Explicit quit must save window geometry: '+JSON.stringify(prefs));}
  await log.close();
  let stopped=!record;
  if(record){for(let i=0;i<50;i++){try{const health=await(await fetch('http://'+record.address+'/api/health',{signal:AbortSignal.timeout(200)})).json();if(health.instance_id!==record.instance_id){stopped=true;break;}await pause(100);}catch{stopped=true;break;}}}
  assert(dir.startsWith(parent+path.sep)&&path.basename(dir).startsWith('desktop-smoke-'));
  if(process.platform!=='win32'){assert.match(path.basename(cacheDir(state)),/^[a-f0-9]{64}$/);await rm(cacheDir(state),{recursive:true,force:true});}
  await rm(dir,{recursive:true,force:true,maxRetries:20,retryDelay:100});
  assert.equal(child.exitCode,0,'Host must quit normally');assert(stopped,'Owned service must exit');
}
