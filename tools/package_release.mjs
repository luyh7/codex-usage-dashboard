#!/usr/bin/env node
import {execFileSync} from 'node:child_process';
import {readFile,writeFile,mkdir,copyFile,chmod} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {platformKey,binaryName} from '../skills/codex-usage-dashboard/scripts/install_runtime.mjs';
import {checkRelease,platforms} from './check_release.mjs';
const root=path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const args=process.argv.slice(2);const outputFlag=args.indexOf('--output');
const output=outputFlag<0?path.join(root,'dist/release'):path.resolve(args[outputFlag+1]);
const config=await checkRelease(process.env.RELEASE_TAG);
const key=platformKey();
if(!platforms.includes(key)) throw Error(`Unsupported release platform: ${key}`);
if(process.env.RELEASE_PLATFORM && process.env.RELEASE_PLATFORM!==key) throw Error(`Runner architecture mismatch: expected ${process.env.RELEASE_PLATFORM}, got ${key}`);
if(!args.includes('--skip-build')) {
  const npm=process.platform==='win32'?'npm.cmd':'npm';
  for(const command of [['ci'],['run','build']]) execFileSync(npm,command,{cwd:path.join(root,'web'),stdio:'inherit',shell:process.platform==='win32'});
  execFileSync('cargo',['build','--release','--locked','--bin','codex-usage-dashboard'],{cwd:root,stdio:'inherit',env:{...process.env,COUSASH_REQUIRE_WEB:'1'}});
}
const binary=path.join(process.env.CARGO_TARGET_DIR || path.join(root,'target'),'release',binaryName());
const info=JSON.parse(execFileSync(binary,['--build-info'],{encoding:'utf8'}));
if(info.ui!=='svelte'||info.version!==config.version) throw Error('Build the current dashboard before packaging; runtime and skill must match');
const name=`codex-usage-dashboard-${info.version}-${key}-${info.build_id.slice(0,12)}${process.platform==='win32'?'.exe':''}`;
await mkdir(output,{recursive:true});await copyFile(binary,path.join(output,name));await chmod(path.join(output,name),0o755);
const sha256=createHash('sha256').update(await readFile(binary)).digest('hex');
const manifest={version:info.version,protocol_version:1,ui:info.ui,platforms:{[key]:{file:name,sha256,build_id:info.build_id}}};
await writeFile(path.join(output,'manifest.json'),JSON.stringify(manifest,null,2)+'\n');
await writeFile(path.join(output,'SHA256SUMS'),`${sha256}  ${name}\n`);
console.log(JSON.stringify(manifest));
