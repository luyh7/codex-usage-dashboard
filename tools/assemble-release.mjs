// Combine verified CLI and signed desktop staging directories into one version.
import assert from 'node:assert/strict';
import {readFile, readdir, mkdir, copyFile, writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {checkRelease, platforms} from './check_release.mjs';
import {nativeAssetName} from './release-names.mjs';

const digest = bytes => createHash('sha256').update(bytes).digest('hex');
export async function assembleRelease(native, desktop, output, tag) {
  const {version, repository} = await checkRelease(tag);
  const staged = new Map();
  for (const dir of [native, desktop]) {
    for (const entry of await readdir(dir, {withFileTypes: true})) {
      assert(entry.isFile(), 'Staging directories must contain only release files');
      if (entry.name === 'SHA256SUMS') continue;
      assert(!staged.has(entry.name), `Duplicate release asset: ${entry.name}`);
      staged.set(entry.name, {source: path.join(dir, entry.name), bytes: await readFile(path.join(dir, entry.name))});
    }
    for (const line of (await readFile(path.join(dir, 'SHA256SUMS'), 'utf8')).trim().split('\n')) {
      const match = line.match(/^([a-f0-9]{64})  ([A-Za-z0-9][A-Za-z0-9._-]+)$/);
      assert(match, 'Invalid checksum entry');
      const bytes = await readFile(path.join(dir, match[2]));
      assert.equal(digest(bytes), match[1], `Checksum mismatch: ${match[2]}`);
    }
  }
  const nativeManifest = JSON.parse(staged.get('manifest.json')?.bytes.toString() || '{}');
  const update = JSON.parse(staged.get('latest.json')?.bytes.toString() || '{}');
  assert.equal(nativeManifest.version, version, 'Native version mismatch');
  assert.equal(nativeManifest.protocol_version, 1);
  assert.equal(nativeManifest.ui, 'svelte');
  assert.deepEqual(Object.keys(nativeManifest.platforms).sort(), [...platforms].sort());
  for (const [key, entry] of Object.entries(nativeManifest.platforms)) {
    assert.equal(entry.file, nativeAssetName(version, key));
    assert.match(entry.build_id, /^[a-f0-9]{64}$/);
    assert.equal(digest(staged.get(entry.file)?.bytes || Buffer.alloc(0)), entry.sha256);
  }
  assert.equal(update.version, version, 'Desktop version mismatch');
  assert(Object.keys(update.platforms).length, 'Desktop updater platforms are required');
  const base = `https://github.com/${repository}/releases/download/v${version}/`;
  for (const entry of Object.values(update.platforms)) {
    assert(entry.url.startsWith(base), 'Updater assets must belong to the same version Release');
    const name = decodeURIComponent(entry.url.slice(base.length));
    assert(/^[A-Za-z0-9][A-Za-z0-9._-]+$/.test(name), 'Unsafe updater asset name');
    assert(staged.has(name), `Missing updater artifact: ${name}`);
    assert.equal(staged.get(name + '.sig')?.bytes.toString().trim(), entry.signature, 'Updater signature mismatch');
  }
  await mkdir(output, {recursive: true});
  assert.equal((await readdir(output)).length, 0, 'Output directory must be empty');
  const sums = [];
  for (const [name, entry] of [...staged].sort(([a], [b]) => a.localeCompare(b))) {
    await copyFile(entry.source, path.join(output, name));
    sums.push(`${digest(entry.bytes)}  ${name}`);
  }
  await writeFile(path.join(output, 'SHA256SUMS'), sums.join('\n') + '\n');
  return {version, assets: [...staged.keys(), 'SHA256SUMS'].sort()};
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [native, desktop, output, tag] = process.argv.slice(2);
  assert(native && desktop && output, 'Usage: node tools/assemble-release.mjs NATIVE DESKTOP OUTPUT [vVERSION]');
  console.log(JSON.stringify(await assembleRelease(native, desktop, output, tag)));
}
