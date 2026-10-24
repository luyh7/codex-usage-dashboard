// Validate the complete release before copying anything into the publish directory.
import assert from 'node:assert/strict';
import {readFile, writeFile, readdir, mkdir, copyFile} from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {checkRelease, platforms} from './check_release.mjs';
import {nativeAssetName} from './release-names.mjs';

export async function mergeRelease(input, output, tag) {
  const {version} = await checkRelease(tag);
  const combined = {version, protocol_version: 1, ui: 'svelte', platforms: {}};
  const artifacts = [];
  async function walk(dir) {
    for (const entry of await readdir(dir, {withFileTypes: true})) {
      const file = path.join(dir, entry.name);
      if (entry.isDirectory()) await walk(file);
      else if (entry.name === 'manifest.json') {
        const manifest = JSON.parse(await readFile(file, 'utf8'));
        assert.equal(manifest.version, version, 'Manifest version mismatch');
        assert.equal(manifest.protocol_version, 1, 'Manifest protocol mismatch');
        assert.equal(manifest.ui, 'svelte', 'Manifest dashboard mismatch');
        for (const [key, value] of Object.entries(manifest.platforms)) {
          assert(platforms.includes(key), `Unsupported platform: ${key}`);
          assert(!combined.platforms[key], `Duplicate platform: ${key}`);
          assert.match(value.build_id, /^[a-f0-9]{64}$/, 'Invalid build identity');
          assert.match(value.sha256, /^[a-f0-9]{64}$/, 'Invalid checksum');
          const expected = nativeAssetName(version, key);
          assert.equal(value.file, expected, 'Invalid artifact filename');
          const source = path.join(dir, value.file);
          const bytes = await readFile(source);
          assert.equal(createHash('sha256').update(bytes).digest('hex'), value.sha256, 'Artifact checksum mismatch');
          combined.platforms[key] = value;
          artifacts.push({source, ...value});
        }
      }
    }
  }
  await walk(input);
  assert.deepEqual(Object.keys(combined.platforms).sort(), [...platforms].sort(), 'Missing native platform artifacts');
  await mkdir(output, {recursive: true});
  assert.equal((await readdir(output)).length, 0, 'Output directory must be empty; do not mix release candidates');
  artifacts.sort((a, b) => a.file.localeCompare(b.file));
  for (const entry of artifacts) await copyFile(entry.source, path.join(output, entry.file));
  await writeFile(path.join(output, 'manifest.json'), JSON.stringify(combined, null, 2) + '\n');
  await writeFile(path.join(output, 'SHA256SUMS'), artifacts.map(a => `${a.sha256}  ${a.file}`).join('\n') + '\n');
  return combined;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [input, output, tag] = process.argv.slice(2);
  if (!input || !output) throw Error('Usage: node tools/merge_release.mjs INPUT OUTPUT [vVERSION]');
  const manifest = await mergeRelease(input, output, tag);
  console.log(`Assembled ${manifest.version}: ${Object.keys(manifest.platforms).join(', ')}`);
}
