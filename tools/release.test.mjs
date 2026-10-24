import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp, mkdir, readFile, writeFile, rm, readdir, cp} from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import {createHash} from 'node:crypto';
import {checkRelease, platforms} from './check_release.mjs';
import {mergeRelease} from './merge_release.mjs';
import {nativeAssetName} from './release-names.mjs';

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'dashboard-release-'));
  t.after(() => rm(root, {recursive: true, force: true}));
  const input = path.join(root, 'input'), output = path.join(root, 'output');
  const {version} = await checkRelease();
  for (const key of platforms) {
    const dir = path.join(input, key);
    await mkdir(dir, {recursive: true});
    const bytes = Buffer.from(`fixture:${key}`);
    const sha256 = createHash('sha256').update(bytes).digest('hex');
    const file = nativeAssetName(version, key);
    await writeFile(path.join(dir, file), bytes);
    await writeFile(path.join(dir, 'manifest.json'), JSON.stringify({version, protocol_version: 1, ui: 'svelte', platforms: {[key]: {file, sha256, build_id: sha256}}}));
  }
  return {input, output, version};
}

test('assemble all supported platforms and checksums without publishing', async t => {
  const {input, output, version} = await fixture(t);
  const manifest = await mergeRelease(input, output, `v${version}`);
  assert.deepEqual(Object.keys(manifest.platforms).sort(), [...platforms].sort());
  assert.equal((await readdir(output)).length, platforms.length + 2);
  for (const entry of Object.values(manifest.platforms)) {
    assert.equal(createHash('sha256').update(await readFile(path.join(output, entry.file))).digest('hex'), entry.sha256);
    assert((await readFile(path.join(output, 'SHA256SUMS'), 'utf8')).includes(`${entry.sha256}  ${entry.file}\n`));
  }
  await assert.rejects(mergeRelease(input, output), /must be empty/);
});

test('reject incomplete releases, including either missing Mac architecture', async t => {
  for (const missing of platforms) {
    const {input, output} = await fixture(t);
    await rm(path.join(input, missing), {recursive: true});
    await assert.rejects(mergeRelease(input, output), /Missing native platform/);
    await assert.rejects(readdir(output), {code: 'ENOENT'});
  }
});

test('reject duplicate platforms and tag/version mismatch', async t => {
  const {input, output} = await fixture(t);
  await assert.rejects(mergeRelease(input, output, 'v0.0.0'), /Tag must match/);
  await cp(path.join(input, platforms[0]), path.join(input, 'duplicate'), {recursive: true});
  await assert.rejects(mergeRelease(input, output), /Duplicate platform/);
});

test('reject tampered downloads before writing a candidate', async t => {
  const {input, output} = await fixture(t);
  const dir = path.join(input, platforms[0]);
  const manifest = JSON.parse(await readFile(path.join(dir, 'manifest.json'), 'utf8'));
  await writeFile(path.join(dir, manifest.platforms[platforms[0]].file), 'corrupted');
  await assert.rejects(mergeRelease(input, output), /checksum mismatch/);
  await assert.rejects(readdir(output), {code: 'ENOENT'});
});

test('reject unsafe filenames, unknown platforms, and incompatible manifests', async t => {
  for (const mutation of ['path', 'platform', 'version', 'protocol', 'ui', 'build']) {
    const {input, output} = await fixture(t);
    const file = path.join(input, platforms[0], 'manifest.json');
    const manifest = JSON.parse(await readFile(file, 'utf8'));
    if (mutation === 'path') manifest.platforms[platforms[0]].file = '../outside';
    if (mutation === 'platform') manifest.platforms = {'linux-unknown': manifest.platforms[platforms[0]]};
    if (mutation === 'version') manifest.version = '0.0.0';
    if (mutation === 'protocol') manifest.protocol_version = 2;
    if (mutation === 'ui') manifest.ui = 'classic';
    if (mutation === 'build') manifest.platforms[platforms[0]].build_id = 'invalid';
    await writeFile(file, JSON.stringify(manifest));
    await assert.rejects(mergeRelease(input, output));
    await assert.rejects(readdir(output), {code: 'ENOENT'});
  }
});
