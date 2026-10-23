import{test}from'node:test';import assert from'node:assert/strict';
import{mkdtemp,mkdir,writeFile,readFile,rm}from'node:fs/promises';import{tmpdir}from'node:os';import path from'node:path';
import{stageUpdates}from'./desktop-update.mjs';
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
