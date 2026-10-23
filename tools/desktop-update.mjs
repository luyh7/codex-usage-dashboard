// Assemble signed, architecture-specific update artifacts. This does not publish.
import {readFile,writeFile,mkdir,readdir,copyFile} from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
const targets=[
  ['x86_64-pc-windows-msvc','windows-x86_64','windows-x64','.exe'],
  ['aarch64-apple-darwin','darwin-aarch64','darwin-arm64','.app.tar.gz'],
  ['x86_64-apple-darwin','darwin-x86_64','darwin-x64','.app.tar.gz'],
  ['x86_64-unknown-linux-gnu','linux-x86_64','linux-x64','.AppImage'],
];
async function files(dir){const rows=await readdir(dir,{withFileTypes:true});return(await Promise.all(rows.map(e=>e.isDirectory()?files(path.join(dir,e.name)):path.join(dir,e.name)))).flat();}
export async function stageUpdates(root,version,base,notes,output){
  if(!/^\d+\.\d+\.\d+(?:-[\da-z.-]+)?$/i.test(version))throw Error('Invalid release version');
  const url=new URL(base.endsWith('/')?base:base+'/');
  if(url.protocol!=='https:'&&!(url.protocol==='http:'&&['127.0.0.1','localhost','[::1]'].includes(url.hostname)))throw Error('Use HTTPS for release artifacts');
  await mkdir(output,{recursive:true});const platforms={};const checksums=[];
  for(const[target,key,label,suffix]of targets){
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
    await copyFile(artifacts[0],path.join(output,name));await writeFile(path.join(output,name+'.sig'),signature+'\n');
    const digest=createHash('sha256').update(await readFile(artifacts[0])).digest('hex');checksums.push(`${digest}  ${name}`);
    platforms[key]={url:new URL(encodeURIComponent(name),url).href,signature};
  }
  const manifest={version,notes,pub_date:new Date().toISOString(),platforms};
  await writeFile(path.join(output,'latest.json'),JSON.stringify(manifest,null,2)+'\n');
  await writeFile(path.join(output,'SHA256SUMS'),checksums.join('\n')+'\n');return manifest;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(path.resolve(process.argv[1])).href){
  const[root,version,base,notes,output]=process.argv.slice(2);
  if(!output)throw Error('Usage: node tools/desktop-update.mjs <downloaded-artifacts> <version> <base-url> <notes-file> <output-directory>');
  await stageUpdates(root,version,base,await readFile(notes,'utf8'),output);
}
