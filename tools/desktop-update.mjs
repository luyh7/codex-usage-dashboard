// Assemble signed, architecture-specific update artifacts. This does not publish.
import {readFile,writeFile,mkdir,readdir,copyFile,open,unlink} from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
import {parseArgs} from 'node:util';
const targets=[
  ['x86_64-pc-windows-msvc','windows-x86_64','windows-x64','.exe'],
  ['aarch64-apple-darwin','darwin-aarch64','darwin-arm64','.app.tar.gz'],
  ['x86_64-apple-darwin','darwin-x86_64','darwin-x64','.app.tar.gz'],
  ['x86_64-unknown-linux-gnu','linux-x86_64','linux-x64','.AppImage'],
];
async function files(dir){const rows=await readdir(dir,{withFileTypes:true});return(await Promise.all(rows.map(e=>e.isDirectory()?files(path.join(dir,e.name)):path.join(dir,e.name)))).flat();}
export async function stageUpdates(root,version,base,notes,output,platformKeys=targets.map(([,key])=>key)){
  if(!/^\d+\.\d+\.\d+(?:-[\da-z.-]+)?$/i.test(version))throw Error('Invalid release version');
  const url=new URL(base.endsWith('/')?base:base+'/');
  if(url.protocol!=='https:'&&!(url.protocol==='http:'&&['127.0.0.1','localhost','[::1]'].includes(url.hostname)))throw Error('Use HTTPS for release artifacts');
  if(!Array.isArray(platformKeys)||!platformKeys.length||new Set(platformKeys).size!==platformKeys.length||platformKeys.some(key=>!targets.some(row=>row[1]===key)))throw Error('Select distinct supported update platforms: '+targets.map(([,key])=>key).join(', '));
  const prepared=[];
  for(const[target,key,label,suffix]of targets.filter(([,key])=>platformKeys.includes(key))){
    const dir=path.join(root,'desktop-'+target);const all=await files(dir);
    const builds=all.filter(f=>path.basename(f)==='service-build.json');
    if(builds.length!==1)throw Error(`${label}: expected one service build manifest`);
    const info=JSON.parse(await readFile(builds[0],'utf8'));
    if(info.version!==version||!/^[a-f0-9]{64}$/.test(info.build_id))throw Error(`${label}: mismatched version or missing build identity`);
    const artifacts=all.filter(f=>f.endsWith(suffix)&&!f.endsWith('uninstall.exe'));
    if(artifacts.length!==1)throw Error(`${label}: expected one ${suffix} update artifact`);
    const signature=(await readFile(artifacts[0]+'.sig','utf8')).trim();
    if(!signature||!/^[A-Za-z0-9+/=\r\n]+$/.test(signature))throw Error(`${label}: missing or invalid updater signature`);
    const name=`cousash-${version}-${label}${suffix}`;
    const digest=createHash('sha256').update(await readFile(artifacts[0])).digest('hex');
    prepared.push({key,name,artifact:artifacts[0],signature,digest});
  }
  await mkdir(output,{recursive:true});
  const lockPath=path.join(output,'.staging.lock');
  const lock=await open(lockPath,'wx');
  try{
  if((await readdir(output)).some(name=>name!=='.staging.lock'))throw Error('Use an empty staging directory to avoid mixing release platforms or versions');
  const platforms={};const checksums=[];
  for(const{key,name,artifact,signature,digest}of prepared){
    await copyFile(artifact,path.join(output,name));await writeFile(path.join(output,name+'.sig'),signature+'\n');
    checksums.push(`${digest}  ${name}`);
    platforms[key]={url:new URL(encodeURIComponent(name),url).href,signature};
  }
  const manifest={version,notes,pub_date:new Date().toISOString(),platforms};
  await writeFile(path.join(output,'latest.json'),JSON.stringify(manifest,null,2)+'\n');
  await writeFile(path.join(output,'SHA256SUMS'),checksums.join('\n')+'\n');return manifest;
  }finally{await lock.close();await unlink(lockPath);}
}
if(process.argv[1]&&import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href){
  const{positionals,values}=parseArgs({allowPositionals:true,options:{platform:{type:'string',multiple:true}}});
  const[root,version,base,notes,output]=positionals;
  if(positionals.length!==5)throw Error('Usage: node tools/desktop-update.mjs <downloaded-artifacts> <version> <base-url> <notes-file> <empty-output-directory> [--platform <updater-platform>]...');
  await stageUpdates(root,version,base,await readFile(notes,'utf8'),output,values.platform);
}
