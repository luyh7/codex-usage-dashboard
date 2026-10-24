// Mirrors the public path contract for integration drivers; never reads tokens
// from a shared Unix worktree filesystem.
import path from 'node:path';
import os from 'node:os';
import {realpathSync} from 'node:fs';
import {createHash} from 'node:crypto';
export function cacheDir(root){
  if(process.platform==='win32')return path.join(root,'cache');
  let canonical;try{canonical=realpathSync(root);}catch{canonical=path.resolve(root);}
  const base=process.platform==='darwin'?path.join(os.homedir(),'Library/Caches'):(process.env.XDG_CACHE_HOME||path.join(os.homedir(),'.cache'));
  return path.join(base,'cousash/scopes',createHash('sha256').update(canonical).digest('hex'));
}
export const runtimeDir=root=>process.platform==='win32'?path.join(root,'runtime'):path.join(cacheDir(root),'runtime');
