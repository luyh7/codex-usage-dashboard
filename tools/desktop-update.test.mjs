import{test}from'node:test';import assert from'node:assert/strict';
import{mkdtemp,mkdir,writeFile,readFile,readdir,rm}from'node:fs/promises';import{tmpdir}from'node:os';import path from'node:path';
import{stageUpdates}from'./desktop-update.mjs';
import{spawnSync}from'node:child_process';import{fileURLToPath}from'node:url';
test('update manifests require complete architecture coverage and keep artifacts separate',async t=>{
  const dir=await mkdtemp(path.join(tmpdir(),'cousash-update-'));t.after(()=>rm(dir,{recursive:true,force:true}));
  const rows=[['x86_64-pc-windows-msvc','.exe'],['aarch64-apple-darwin','.app.tar.gz'],['x86_64-apple-darwin','.app.tar.gz'],['x86_64-unknown-linux-gnu','.AppImage']];
  for(const[target,suffix]of rows){const folder=path.join(dir,'desktop-'+target);await mkdir(folder);await writeFile(path.join(folder,'service-build.json'),JSON.stringify({version:'1.0.1',build_id:'a'.repeat(64)}));await writeFile(path.join(folder,'client'+suffix),target);await writeFile(path.join(folder,'client'+suffix+'.sig'),Buffer.from('synthetic-signature').toString('base64'));}
  const out=path.join(dir,'staged');const result=await stageUpdates(dir,'1.0.1','https://example.test/releases/','notes',out);
  assert.equal(Object.keys(result.platforms).length,4);assert.notEqual(result.platforms['darwin-aarch64'].url,result.platforms['darwin-x86_64'].url);
  assert.equal(await readFile(path.join(out,'cousash-1.0.1-darwin-arm64.app.tar.gz'),'utf8'),'aarch64-apple-darwin');
  await assert.rejects(stageUpdates(dir,'1.0.2','https://example.test/','',out),/mismatched version/);
  await assert.rejects(stageUpdates(dir,'1.0.1','http://example.test/','',out),/HTTPS/);
  await rm(path.join(dir,'desktop-x86_64-apple-darwin/client.app.tar.gz.sig'));
  await assert.rejects(stageUpdates(dir,'1.0.1','https://example.test/','',out));
});

async function limitedRelease(t){
  const dir=await mkdtemp(path.join(tmpdir(),'cousash-limited-update-'));t.after(()=>rm(dir,{recursive:true,force:true}));
  for(const[target,suffix]of [['x86_64-pc-windows-msvc','.exe'],['aarch64-apple-darwin','.app.tar.gz']]){
    const folder=path.join(dir,'desktop-'+target);await mkdir(folder);await writeFile(path.join(folder,'service-build.json'),JSON.stringify({version:'1.0.1',build_id:'b'.repeat(64)}));await writeFile(path.join(folder,'client'+suffix),target);await writeFile(path.join(folder,'client'+suffix+'.sig'),Buffer.from('synthetic-signature').toString('base64'));
  }
  return dir;
}
test('explicit CLI platforms stage only the selected release targets',async t=>{
  const dir=await limitedRelease(t),out=path.join(dir,'staged'),notes=path.join(dir,'notes.txt');await writeFile(notes,'Limited stable release');
  const unselected=path.join(dir,'desktop-x86_64-apple-darwin');await mkdir(unselected);await writeFile(path.join(unselected,'client.app.tar.gz'),'unverified development package');
  const result=spawnSync(process.execPath,[fileURLToPath(new URL('./desktop-update.mjs',import.meta.url)),dir,'1.0.1','https://example.test/releases/',notes,out,'--platform','windows-x86_64','--platform','darwin-aarch64'],{encoding:'utf8'});
  assert.equal(result.status,0,result.stderr);
  const manifest=JSON.parse(await readFile(path.join(out,'latest.json'),'utf8'));assert.deepEqual(Object.keys(manifest.platforms),['windows-x86_64','darwin-aarch64']);
  assert.equal(manifest.notes,'Limited stable release');assert.equal((await readFile(path.join(out,'SHA256SUMS'),'utf8')).trim().split('\n').length,2);
  assert.equal(await readFile(path.join(out,'cousash-1.0.1-windows-x64.exe'),'utf8'),'x86_64-pc-windows-msvc');
  await assert.rejects(readFile(path.join(out,'cousash-1.0.1-darwin-x64.app.tar.gz')),/ENOENT/);
});
test('platform selection never silently drops missing targets or accepts invalid selections',async t=>{
  const dir=await limitedRelease(t),out=path.join(dir,'staged');
  for(const selection of [[],['windows-x86_64','windows-x86_64'],['darwin-arm64']])await assert.rejects(stageUpdates(dir,'1.0.1','https://example.test/','',out,selection),/platform/i);
  await assert.rejects(stageUpdates(dir,'1.0.1','https://example.test/','',out,['windows-x86_64','linux-x86_64']),/ENOENT/);
  await rm(path.join(dir,'desktop-aarch64-apple-darwin/client.app.tar.gz.sig'));
  await assert.rejects(stageUpdates(dir,'1.0.1','https://example.test/','',out,['windows-x86_64','darwin-aarch64']),/ENOENT/);
  await assert.rejects(readFile(path.join(out,'cousash-1.0.1-windows-x64.exe')),/ENOENT/);
});
test('staging rejects stale output without changing a previously staged release',async t=>{
  const dir=await limitedRelease(t),out=path.join(dir,'staged');await mkdir(out);await writeFile(path.join(out,'latest.json'),'existing release');await writeFile(path.join(out,'unverified-linux.AppImage'),'preserve');
  await assert.rejects(stageUpdates(dir,'1.0.1','https://example.test/','',out,['windows-x86_64','darwin-aarch64']),/empty.*director/i);
  assert.equal(await readFile(path.join(out,'latest.json'),'utf8'),'existing release');assert.equal(await readFile(path.join(out,'unverified-linux.AppImage'),'utf8'),'preserve');
  await assert.rejects(readFile(path.join(out,'cousash-1.0.1-windows-x64.exe')),/ENOENT/);
});

test('concurrent staging has one winner and never mixes platform artifacts',async t=>{
  const dir=await limitedRelease(t),out=path.join(dir,'staged');await mkdir(out);
  const results=await Promise.allSettled(['windows-x86_64','darwin-aarch64'].map(key=>stageUpdates(dir,'1.0.1','https://example.test/','',out,[key])));
  assert.equal(results.filter(result=>result.status==='fulfilled').length,1);
  assert.equal(results.filter(result=>result.status==='rejected').length,1);
  const winner=results.find(result=>result.status==='fulfilled').value;
  assert.deepEqual(JSON.parse(await readFile(path.join(out,'latest.json'),'utf8')),winner);
  const artifact=decodeURIComponent(new URL(Object.values(winner.platforms)[0].url).pathname.split('/').pop());
  assert.deepEqual((await readdir(out)).sort(),['SHA256SUMS','latest.json',artifact,artifact+'.sig'].sort());
  assert.equal((await readFile(path.join(out,'SHA256SUMS'),'utf8')).trim().split('\n').length,1);
});
test('an existing staging lock is preserved and refuses a second writer',async t=>{
  const dir=await limitedRelease(t),out=path.join(dir,'staged');await mkdir(out);await writeFile(path.join(out,'.staging.lock'),'another writer');
  await assert.rejects(stageUpdates(dir,'1.0.1','https://example.test/','',out,['windows-x86_64']));
  assert.equal(await readFile(path.join(out,'.staging.lock'),'utf8'),'another writer');
  assert.deepEqual(await readdir(out),['.staging.lock']);
});
