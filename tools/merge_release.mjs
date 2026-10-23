// Merge native job manifests for a version-tagged release; verify every artifact.
import {readFile,writeFile,readdir,mkdir,copyFile} from 'node:fs/promises';
import path from 'node:path';
import {createHash} from 'node:crypto';
const [input,output,tag]=process.argv.slice(2);
if(!input||!output||!tag) throw Error('Usage: node tools/merge_release.mjs INPUT OUTPUT vVERSION');
const version=tag.replace(/^v/,'');const combined={version,protocol_version:1,ui:'svelte',platforms:{}};const sums=[];
const config=JSON.parse(await readFile(new URL('../skills/codex-usage-dashboard/native.json',import.meta.url),'utf8'));
if(config.version!==version)throw Error('Tag/skill version mismatch');
await mkdir(output,{recursive:true});
async function walk(dir){for(const entry of await readdir(dir,{withFileTypes:true})){const file=path.join(dir,entry.name);if(entry.isDirectory())await walk(file);else if(entry.name==='manifest.json'){
 const manifest=JSON.parse(await readFile(file,'utf8'));if(manifest.version!==version||manifest.protocol_version!==1||manifest.ui!=='svelte')throw Error('Tag/manifest version or dashboard mismatch');
 for(const [key,value] of Object.entries(manifest.platforms)){
  if(combined.platforms[key])throw Error(`Duplicate platform: ${key}`);
  if(path.basename(value.file)!==value.file)throw Error('Invalid artifact path');
  const source=path.join(dir,value.file);const bytes=await readFile(source);if(createHash('sha256').update(bytes).digest('hex')!==value.sha256)throw Error('Artifact checksum mismatch');
  combined.platforms[key]=value;await copyFile(source,path.join(output,value.file));sums.push(`${value.sha256}  ${value.file}`);
 }
}}}
await walk(input);if(Object.keys(combined.platforms).length<3)throw Error('Missing native platform artifacts');
await writeFile(path.join(output,'manifest.json'),JSON.stringify(combined,null,2)+'\n');await writeFile(path.join(output,'SHA256SUMS'),sums.join('\n')+'\n');
