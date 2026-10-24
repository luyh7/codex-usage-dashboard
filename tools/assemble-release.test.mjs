import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp, mkdir, writeFile, readFile, rm, access} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import path from 'node:path';
import {createHash} from 'node:crypto';
import {assembleRelease} from './assemble-release.mjs';
import {checkRelease, platforms} from './check_release.mjs';
import {nativeAssetName, desktopAssetName} from './release-names.mjs';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
async function fixture(t) {
  const dir = await mkdtemp(path.join(tmpdir(), 'cousash-unified-'));
  t.after(() => rm(dir, {recursive: true, force: true}));
  const native = path.join(dir, 'native'), desktop = path.join(dir, 'desktop'), output = path.join(dir, 'output');
  await mkdir(native); await mkdir(desktop);
  const {version, repository} = await checkRelease(), entries = {}, sums = [];
  for (const key of platforms) {
    const file = nativeAssetName(version, key), bytes = Buffer.from(key);
    await writeFile(path.join(native, file), bytes);
    entries[key] = {file, sha256: sha(bytes), build_id: 'a'.repeat(64)};
    sums.push(`${sha(bytes)}  ${file}`);
  }
  await writeFile(path.join(native, 'manifest.json'), JSON.stringify({version, protocol_version: 1, ui: 'svelte', platforms: entries}));
  await writeFile(path.join(native, 'SHA256SUMS'), sums.join('\n') + '\n');
  const name = desktopAssetName(version, 'windows-x64', '.exe'), signature = 'synthetic-signature', bytes = Buffer.from('desktop');
  const update = {version, platforms: {'windows-x86_64': {url: `https://github.com/${repository}/releases/download/v${version}/${name}`, signature}}};
  await writeFile(path.join(desktop, name), bytes); await writeFile(path.join(desktop, name + '.sig'), signature + '\n');
  await writeFile(path.join(desktop, 'latest.json'), JSON.stringify(update));
  await writeFile(path.join(desktop, 'SHA256SUMS'), `${sha(bytes)}  ${name}\n`);
  return {native, desktop, output, version, name, update};
}
test('one Release has both manifests and one complete checksum list', async t => {
  const f = await fixture(t), result = await assembleRelease(f.native, f.desktop, f.output, `v${f.version}`);
  assert.equal(result.assets.length, 9);
  const sums = (await readFile(path.join(f.output, 'SHA256SUMS'), 'utf8')).trim().split('\n');
  assert.equal(sums.length, 8);
  for (const line of sums) { const [hash, file] = line.split('  '); assert.equal(sha(await readFile(path.join(f.output, file))), hash); }
  assert.equal(nativeAssetName(f.version, 'darwin-arm64'), `cousash-${f.version}-cli-macos-arm64`);
  assert.equal(f.name, `cousash-${f.version}-windows-x64-setup.exe`);
});
test('reject cross-release URLs, missing signatures and version mismatch before writing', async t => {
  for (const mutation of ['url', 'signature', 'version']) {
    const f = await fixture(t);
    if (mutation === 'url') f.update.platforms['windows-x86_64'].url = 'https://example.test/client.exe';
    if (mutation === 'signature') await rm(path.join(f.desktop, f.name + '.sig'));
    if (mutation === 'version') f.update.version = '0.0.0';
    await writeFile(path.join(f.desktop, 'latest.json'), JSON.stringify(f.update));
    await assert.rejects(assembleRelease(f.native, f.desktop, f.output));
    await assert.rejects(access(f.output), {code: 'ENOENT'});
  }
});
test('reject corrupt artifacts and collisions before writing', async t => {
  for (const collision of [false, true]) {
    const f = await fixture(t);
    await writeFile(path.join(f.desktop, collision ? 'manifest.json' : f.name), 'corrupted');
    await assert.rejects(assembleRelease(f.native, f.desktop, f.output), /Duplicate|Checksum/);
    await assert.rejects(access(f.output), {code: 'ENOENT'});
  }
});
