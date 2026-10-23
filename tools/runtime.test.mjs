import {test} from 'node:test';
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdir,mkdtemp,readFile,writeFile,rm,rename,copyFile} from 'node:fs/promises';
import http from 'node:http';
import {runtimeDir,cacheDir} from './runtime-paths.mjs';
import path from 'node:path';
import {gunzipSync} from 'node:zlib';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const binary=path.resolve(root,process.env.AUDIT_BINARY||`target/debug/codex-usage-dashboard${process.platform==='win32'?'.exe':''}`);
function cli(args,env,json=true,executable=binary) {return new Promise((resolve,reject)=>{const child=spawn(executable,args,{env,windowsHide:true,stdio:['ignore','pipe','pipe']});let out='',err='';const timeout=setTimeout(()=>{child.kill();reject(Error(`command timeout: ${args.join(' ')} ${err}`));},20000);child.stdout.on('data',d=>out+=d);child.stderr.on('data',d=>err+=d);child.on('error',reject);child.on('close',code=>{clearTimeout(timeout);try{resolve({code,value:json?JSON.parse(out):null,out,err});}catch(e){reject(Error(`${e}: ${out} ${err}`));}});});}
const pause=ms=>new Promise(r=>setTimeout(r,ms));
test('managed lifecycle, concurrency, settings, timezone, exports and corrupt records',async t=>{
  await mkdir(path.join(root,'output/runtime-tests'),{recursive:true});
  const dir=await mkdtemp(path.join(root,'output/runtime-tests/case-'));
  const home=path.join(dir,'日志 with spaces');const state=path.join(dir,'state');
  await mkdir(path.join(home,'sessions'),{recursive:true});await mkdir(state);
  const runtime=runtimeDir(state);
  await writeFile(path.join(home,'sessions/test.jsonl'),[
    {type:'session_meta',payload:{id:'desktop-fixture',model:'gpt-5',cwd:'/synthetic/project'}},
    {timestamp:'2026-10-01T23:59:00Z',type:'event_msg',payload:{type:'user_message',message:'SYNTHETIC_PRIVATE_MESSAGE'}},
    {timestamp:'2026-10-01T23:59:01Z',type:'event_msg',payload:{type:'token_count',info:{total_token_usage:{input_tokens:100,output_tokens:20,total_tokens:120}}}},
  ].map(JSON.stringify).join('\n')+'\n');
  const env={...process.env,CODEX_HOME:home,COUSASH_CONFIG_DIR:state,TZ:'UTC'};
  const record=async()=>JSON.parse(await readFile(path.join(runtime,'instance.json'),'utf8'));
  async function stop(){try{const r=await record();const health=await(await fetch('http://'+r.address+'/api/health')).json();if(health.instance_id===r.instance_id)await fetch('http://'+r.address+'/api/shutdown',{method:'POST',headers:{Authorization:'Bearer '+r.shutdown_token}});await pause(500);}catch{}}
  t.after(async()=>{await stop();if(process.platform!=='win32'){assert.match(path.basename(cacheDir(state)),/^[a-f0-9]{64}$/);await rm(cacheDir(state),{recursive:true,force:true});}await rm(dir,{recursive:true,force:true});});
  assert.equal((await cli(['status','--json'],env)).value.data.state,'stopped');
  assert.equal((await cli(['open','--json'],env)).value.error.code,'desktop_not_installed');
  const [first,second]=await Promise.all([cli(['summary','--period','all','--json'],env),cli(['summary','--period','all','--json'],env)]);
  for(const result of [first,second]){assert.equal(result.code,0,result.out);assert.equal(result.value.data.summary.usage.total_tokens,120);}
  const r=await record();const base='http://'+r.address;
  const launched=await cli(['--launch','--no-browser','--no-auto-windows'],env,false);
  assert.equal(launched.code,0,launched.err);assert.equal(launched.out.trim(),base+'/');assert.equal((await record()).pid,r.pid);
  assert.notEqual(r.address.split(':').at(-1),'0');
  const status=(await cli(['status','--json'],env)).value;
  assert.equal(status.data.pid,r.pid);assert(!JSON.stringify(status).includes(r.shutdown_token));
  const token=(await(await fetch(base+'/api/security/token')).json()).token;
  const save=value=>fetch(base+'/api/settings',{method:'POST',headers:{'Content-Type':'application/json','x-codex-write-token':token},body:JSON.stringify(value)});
  assert.equal((await fetch(base+'/api/settings',{method:'POST',headers:{'Content-Type':'application/json'},body:'{}'})).status,403);
  assert.equal((await save({schema_version:1,timezone:'invalid'})).status,400);
  assert.equal((await save({schema_version:1,language:'zh',timezone:'Asia/Shanghai',codex_homes:[]})).status,200);
  assert.equal((await save({language:'en'})).status,200);
  assert.equal((await(await fetch(base+'/api/settings')).json()).timezone,'Asia/Shanghai');
  const prefs=path.join(state,'preferences.json');await rename(prefs,prefs+'.backup');await mkdir(prefs);
  assert.equal((await save({language:'zh'})).status,500);
  await rm(prefs,{recursive:true});await rename(prefs+'.backup',prefs);
  // Corrupting a record cannot start a second writer or stop its genuine owner.
  await writeFile(path.join(runtime,'instance.json'),'{');
  assert.equal((await cli(['summary','--period','all','--json'],env)).code,1);
  assert.equal((await(await fetch(base+'/api/health')).json()).pid,r.pid);
  await writeFile(path.join(runtime,'instance.json'),JSON.stringify(r));
  const summary=await cli(['summary','--period','custom','--start','2026-10-02','--end','2026-10-02','--json'],env);
  assert.equal(summary.code,0,summary.out);assert.equal(summary.value.data.summary.usage.total_tokens,120);
  const full=path.join(dir,'完整 snapshot.json.gz'),redacted=path.join(dir,'shared.json.gz');
  assert.equal((await cli(['export','--output',full,'--json'],env)).code,0);
  assert.equal((await cli(['export','--output',redacted,'--redact','--json'],env)).code,0);
  assert(gunzipSync(await readFile(full)).toString().includes('SYNTHETIC_PRIVATE_MESSAGE'));
  assert(!gunzipSync(await readFile(redacted)).toString().includes('SYNTHETIC_PRIVATE_MESSAGE'));
  await writeFile(path.join(dir,'keep.json'),'keep');
  assert.equal((await cli(['export','--output',path.join(dir,'missing','file.json'),'--json'],env)).code,1);
  assert.equal(await readFile(path.join(dir,'keep.json'),'utf8'),'keep');
  const other=path.join(dir,path.basename(binary));await copyFile(binary,other);
  assert.equal((await cli(['prepare-install','--json'],env,true,other)).code,0);
  assert.equal((await(await fetch(base+'/api/health')).json()).pid,r.pid);
  await stop();
  await mkdir(runtime,{recursive:true});await writeFile(path.join(runtime,'instance.json'),'{');
  assert.equal((await cli(['summary','--period','all','--json'],env)).code,0);
  assert.equal((await(await fetch('http://'+(await record()).address+'/api/settings')).json()).timezone,'Asia/Shanghai');
  // A forged loopback record is never granted shutdown authority.
  await stop();let shutdowns=0;const unknown=http.createServer((request,response)=>{
    if(request.url==='/api/shutdown')shutdowns++;
    response.setHeader('content-type','application/json');response.end(JSON.stringify({app:'codex-usage-dashboard',protocol_version:1,pid:process.pid,instance_id:r.instance_id,build_id:r.build_id,config_scope:'wrong-scope',index:{phase:'ready'}}));
  });await new Promise(resolve=>unknown.listen(0,'127.0.0.1',resolve));t.after(()=>new Promise(resolve=>unknown.close(resolve)));
  await writeFile(path.join(runtime,'instance.json'),JSON.stringify({...r,address:'127.0.0.1:'+unknown.address().port,pid:process.pid}));
  assert.equal((await cli(['summary','--period','all','--json'],env)).code,0);assert.equal(shutdowns,0);
  assert(unknown.listening);
});
