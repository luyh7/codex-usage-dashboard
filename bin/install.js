#!/usr/bin/env node
"use strict";
const fs=require('fs');
const path=require('path');
const os=require('os');
const cp=require('child_process');
const {pathToFileURL}=require('url');
const name='codex-usage-dashboard';
const root=path.resolve(__dirname,'..');
const source=path.join(root,'skills',name);
// Current Codex user skill location; CODEX_HOME remains an explicit override.
const target=path.join(process.env.CODEX_HOME?path.join(process.env.CODEX_HOME,'skills'):path.join(os.homedir(),'.agents','skills'),name);
function copy(src,dest) {
  if(fs.statSync(src).isDirectory()) {
    fs.mkdirSync(dest,{recursive:true});
    for(const entry of fs.readdirSync(src)) if(!['runtime','__pycache__','.DS_Store'].includes(entry)) copy(path.join(src,entry),path.join(dest,entry));
  } else fs.copyFileSync(src,dest);
}
async function main() {
  const args=process.argv.slice(2);const flag=args.indexOf('--runtime-manifest');
  if(flag>=0 && (!args[flag+1]||args[flag+1].startsWith('--'))) throw Error('--runtime-manifest requires a manifest path or HTTPS URL');
  const {installRuntime}=await import(pathToFileURL(path.join(source,'scripts/install_runtime.mjs')));
  // Finish artifact verification before replacing the installed skill.
  const binary=await installRuntime({manifest:flag<0?undefined:args[flag+1],offline:args.includes('--offline'),rollback:args.includes('--rollback-native'),runtimeRoot:path.join(target,'runtime')});
  copy(source,target);
  for(const alias of ['cousash-open','cousash-export']) fs.rmSync(path.join(path.dirname(target),alias),{recursive:true,force:true});
  console.log(`Installed ${name}: ${target}\nInvoke $codex-usage-dashboard in Codex, or run:\n  "${binary}"`);
  if(args.includes('--shortcut')) cp.execFileSync(binary,['--shortcut'],{stdio:'inherit'});
  if(args.includes('--open')) cp.execFileSync(binary,[],{stdio:'inherit'});
}
main().catch(error=>{console.error(`Installation failed: ${error.message}\nA matching Rust executable is required. For a local build, use --runtime-manifest /path/to/manifest.json --offline.`);process.exitCode=1});
