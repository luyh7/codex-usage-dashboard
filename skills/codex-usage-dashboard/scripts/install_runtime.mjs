#!/usr/bin/env node
// Installation only. The installed executable handles every normal launch.
import {createHash, randomBytes} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {readFile, writeFile, mkdir, rename, chmod, rm} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const skillRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
export const platformKey = () => `${process.platform === 'win32' ? 'windows' : process.platform}-${process.arch}`;
export const binaryName = () => `codex-usage-dashboard${process.platform === 'win32' ? '.exe' : ''}`;
const hash = data => createHash('sha256').update(data).digest('hex');
const exists = async p => {try {await readFile(p); return true} catch(e) {if(e.code === 'ENOENT') return false; throw e}};
async function atomic(file, bytes, mode = 0o600) {
  const temp = file + '.' + randomBytes(8).toString('hex') + '.tmp';
  try {await writeFile(temp, bytes, {mode}); await rename(temp, file)} finally {await rm(temp, {force:true})}
}
async function download(url, limit) {
  if (!url.startsWith('https://')) throw Error('Native downloads require HTTPS');
  const response = await fetch(url, {signal:AbortSignal.timeout(60000)});
  if (!response.ok) throw Error(`Download failed (${response.status}): ${url}`);
  if (!response.url.startsWith('https://')) throw Error('Insecure download redirect');
  const chunks=[]; let total=0;
  for await (const chunk of response.body) {
    total += chunk.length;
    if (total > limit) throw Error('Native download exceeds size limit');
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
}
function entryFor(manifest) {
  const entry = manifest.platforms?.[platformKey()];
  if (!entry) throw Error(`No native artifact for ${platformKey()}; install a release built for this platform.`);
  if (manifest.protocol_version !== 1 || typeof manifest.version !== 'string' ||
      !/^[a-zA-Z0-9][a-zA-Z0-9._-]+$/.test(entry.file) ||
      !/^[a-f0-9]{64}$/.test(entry.sha256) || !/^[a-f0-9]{64}$/.test(entry.build_id)) throw Error('Invalid native manifest');
  return entry;
}
async function verify(binary, manifest) {
  const entry = entryFor(manifest);
  if (hash(await readFile(binary)) !== entry.sha256) throw Error('Native artifact checksum mismatch');
  await chmod(binary, 0o755);
  const info = JSON.parse(execFileSync(binary, ['--build-info'], {encoding:'utf8', timeout:10000, windowsHide:true}));
  if (info.version !== manifest.version || info.build_id !== entry.build_id) throw Error('Native artifact build identity mismatch');
  if (info.ui !== 'svelte') throw Error('Native artifact does not contain the current dashboard; the classic runtime cannot be installed');
}
async function cleanup(runtimeRoot) {
  const scripts = path.join(path.dirname(runtimeRoot), 'scripts');
  for (const obsolete of ['codex_usage_dashboard.py','open_dashboard.py','dashboard_runtime.py','install_runtime.py','install_desktop_shortcut.py','export_snapshot.py','__pycache__']) {
    await rm(path.join(scripts, obsolete), {recursive:true, force:true});
  }
}
export async function ensureRuntime() {
  const config = JSON.parse(await readFile(path.join(skillRoot, 'native.json'), 'utf8'));
  const runtimeRoot = path.join(skillRoot, 'runtime');
  const binary = path.join(runtimeRoot, binaryName());
  try {
    const manifest = JSON.parse(await readFile(path.join(runtimeRoot, 'manifest.json'), 'utf8'));
    if (manifest.version !== config.version) throw Error('Installed runtime needs an update');
    await verify(binary, manifest);
  } catch {
    return installRuntime();
  }
  await cleanup(runtimeRoot);
  return binary;
}
export async function installRuntime({manifest: source, offline = false, rollback = false, runtimeRoot = path.join(skillRoot,'runtime')} = {}) {
  await mkdir(runtimeRoot, {recursive:true, mode:0o700});
  const current = path.join(runtimeRoot,'manifest.json');
  const previous = path.join(runtimeRoot,'manifest.previous.json');
  let manifest, bytes, expectedVersion;
  if (rollback) {
    manifest = JSON.parse(await readFile(previous,'utf8'));
    bytes = await readFile(path.join(runtimeRoot,entryFor(manifest).file));
  } else {
    if (!source) {
      if (offline) throw Error('--offline requires --manifest pointing to a local release manifest');
      const config = JSON.parse(await readFile(path.join(skillRoot,'native.json'),'utf8'));
      expectedVersion = config.version;
      source = `https://github.com/${config.repository}/releases/download/v${config.version}/manifest.json`;
    }
    const remote = source.startsWith('https://');
    if (offline && remote) throw Error('Offline installation requires local files');
    manifest = JSON.parse(remote ? await download(source, 1024*1024) : await readFile(source,'utf8'));
    if (expectedVersion && manifest.version !== expectedVersion) throw Error('Published runtime version does not match this skill');
    const entry = entryFor(manifest);
    bytes = remote ? await download(new URL(entry.file,source).href,256*1024*1024) : await readFile(path.join(path.dirname(path.resolve(source)),entry.file));
  }
  const entry = entryFor(manifest);
  if (hash(bytes) !== entry.sha256) throw Error('Native artifact checksum mismatch; existing installation is unchanged');
  const candidate = path.join(runtimeRoot,`.candidate-${randomBytes(8).toString('hex')}${process.platform==='win32'?'.exe':''}`);
  const stable = path.join(runtimeRoot,binaryName());
  let oldManifest, oldBinary, replaced=false;
  try {
    await writeFile(candidate,bytes,{mode:0o755}); await verify(candidate,manifest);
    if (await exists(current)) oldManifest=await readFile(current);
    if (await exists(stable)) {
      oldBinary=await readFile(stable);
      // The verified new launcher authenticates any managed Rust instance before
      // replacing an executable that Windows might still have open.
      execFileSync(candidate,['--stop'],{stdio:'pipe',timeout:20000,windowsHide:true});
    }
    await atomic(path.join(runtimeRoot,entry.file),bytes,0o755);
    await atomic(stable,bytes,0o755); replaced=true;
    if (oldManifest) await atomic(previous,oldManifest);
    await atomic(current,JSON.stringify(manifest,null,2)+'\n');
    await cleanup(runtimeRoot);
    return stable;
  } catch(error) {
    if (replaced) {
      if (oldBinary) await atomic(stable,oldBinary,0o755); else await rm(stable,{force:true});
      if (oldManifest) await atomic(current,oldManifest); else await rm(current,{force:true});
    }
    throw error;
  } finally {await rm(candidate,{force:true})}
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args=process.argv.slice(2); const index=args.indexOf('--manifest');
  try {
    if(index>=0 && (!args[index+1] || args[index+1].startsWith('--'))) throw Error('--manifest requires a path or HTTPS URL');
    console.log(await (args.includes('--ensure')
      ? ensureRuntime()
      : installRuntime({manifest:index>=0?args[index+1]:undefined,offline:args.includes('--offline'),rollback:args.includes('--rollback')})));
  } catch(error) {
    console.error(`Native installation failed: ${error.message}\nInstall a published native release, or build from source and pass --manifest /path/to/manifest.json --offline. No alternative backend is installed.`);
    process.exitCode=1;
  }
}
