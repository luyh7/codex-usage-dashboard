import {spawnSync} from 'node:child_process';
import {mkdirSync,copyFileSync,writeFileSync,readFileSync,existsSync,readdirSync,statSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const root=fileURLToPath(new URL('../',import.meta.url));
const desktop=path.join(root,'desktop');
const nativeTarget=path.resolve(root,process.env.COUSASH_NATIVE_TARGET_DIR||'target');
const desktopTarget=path.resolve(desktop,process.env.COUSASH_DESKTOP_TARGET_DIR||'target');
function run(command,args,cwd=root,targetDir=nativeTarget){
  if(command==='npm'&&process.platform==='win32'){
    const cli=process.env.npm_execpath||path.join(path.dirname(process.execPath),'node_modules/npm/bin/npm-cli.js');
    if(!existsSync(cli))throw Error('npm CLI unavailable; run this command through npm');
    args=[cli,...args];command=process.execPath;
  }
  const r=spawnSync(command,args,{cwd,stdio:'inherit',env:{...process.env,CARGO_TARGET_DIR:targetDir}});
  if(r.status!==0)throw new Error(`${command} exited ${r.status}`);
}
const action=process.argv[2]||'build';
const release=action==='bundle'||process.argv.includes('--release');
const target=process.env.DESKTOP_TARGET||spawnSync('rustc',['-vV'],{encoding:'utf8'}).stdout.match(/host: (.+)/)[1];
if(!['x86_64-pc-windows-msvc','x86_64-apple-darwin','aarch64-apple-darwin','x86_64-unknown-linux-gnu'].includes(target))throw Error('Unsupported desktop target: '+target);
const extension=target.includes('windows')?'.exe':'';
if(!process.argv.includes('--skip-web'))run('npm',['run','build','--prefix','web']);
run('cargo',['build','--bin','codex-usage-dashboard',...(release?['--release']:[]),'--target',target]);
const binary=path.join(nativeTarget,target,release?'release':'debug','codex-usage-dashboard'+extension);
const info=JSON.parse(spawnSync(binary,['--build-info'],{encoding:'utf8'}).stdout);
mkdirSync(path.join(desktop,'binaries'),{recursive:true});
copyFileSync(binary,path.join(desktop,'binaries',`codex-usage-dashboard-${target}${extension}`));
writeFileSync(path.join(desktop,'service-build.json'),JSON.stringify(info,null,2)+'\n');
writeFileSync(path.join(desktop,`service-build-${target}.json`),JSON.stringify(info,null,2)+'\n');
// Cargo-only development runs resolve the same sibling binary as installed apps.
const output=path.join(desktopTarget,target,release?'release':'debug');
mkdirSync(output,{recursive:true});copyFileSync(binary,path.join(output,'codex-usage-dashboard'+extension));
if(action==='prepare')process.exit(0);
if(action==='bundle'){
  // Tauri CLI is a locked local development dependency, never a user dependency.
  const config=process.env.DESKTOP_RELEASE_CONFIG;
  const cli=path.join(desktop,'node_modules/@tauri-apps/cli/tauri.js');
  if(!existsSync(cli))throw Error('Run npm ci --prefix desktop before bundling');
  run(process.execPath,[cli,'build','--target',target,...(config?['--config',config]:[])],desktop,desktopTarget);
}else{
  run('cargo',['build','--manifest-path','desktop/Cargo.toml','--target',target,...(release?['--release']:[])],root,desktopTarget);
  if(action==='run')run(path.join(output,'cousash-desktop'+extension),[]);
}
writeFileSync(path.join(output,'service-build.json'),JSON.stringify(info,null,2)+'\n');
if(action==='bundle'){
  function files(dir){return readdirSync(dir,{withFileTypes:true}).flatMap(e=>e.isDirectory()?files(path.join(dir,e.name)):[path.join(dir,e.name)]);}
  const product=JSON.parse(readFileSync(path.join(desktop,'tauri.conf.json'),'utf8')).productName;
  const artifacts=files(path.join(output,'bundle')).filter(file=>path.basename(file).startsWith(`${product}_${info.version}_`) && /\.(exe|dmg|deb|AppImage)$/.test(file)).map(file=>({path:file,bytes:statSync(file).size,sha256:createHash('sha256').update(readFileSync(file)).digest('hex')}));
  if(!artifacts.length)throw Error('No current product artifacts found');
  writeFileSync(path.join(output,'artifacts.json'),JSON.stringify({version:info.version,build_id:info.build_id,target,artifacts},null,2)+'\n');
}
