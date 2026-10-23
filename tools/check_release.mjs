import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import path from 'node:path';

const root = new URL('../', import.meta.url);
const json = async file => JSON.parse(await readFile(new URL(file, root), 'utf8'));
export const platforms = ['linux-x64', 'windows-x64', 'darwin-x64', 'darwin-arm64'];

export async function checkRelease(tag) {
  const config = await json('skills/codex-usage-dashboard/native.json');
  const version = config.version;
  assert.match(version, /^\d+\.\d+\.\d+$/, 'Release version must be stable semver');
  for (const file of ['package.json', 'web/package.json', 'web/package-lock.json']) {
    const value = await json(file);
    assert.equal(value.version, version, `${file} version differs from the skill`);
    if (value.packages) assert.equal(value.packages[''].version, version, `${file} root version differs`);
  }
  const cargo = await readFile(new URL('Cargo.toml', root), 'utf8');
  assert.equal(cargo.match(/\[workspace\.package\][\s\S]*?version = "([^"]+)"/)?.[1], version, 'Cargo workspace version differs');
  const lock = await readFile(new URL('Cargo.lock', root), 'utf8');
  for (const name of ['usage-core', 'usage-server']) {
    assert.equal(lock.match(new RegExp(`name = "${name}"\\r?\\nversion = "([^"]+)"`))?.[1], version, `${name} lock version differs`);
  }
  if (tag) assert.equal(tag, `v${version}`, 'Tag must match the release version');
  const notes = await readFile(new URL(`docs/releases/v${version}.md`, root), 'utf8');
  assert(notes.replaceAll('\r\n', '\n').startsWith(`# ${version}\n`), 'Release notes must start with the matching version');
  return config;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const config = await checkRelease(process.argv[2] || process.env.RELEASE_TAG);
  console.log(`Release ${config.version}: versions and release notes match; platforms: ${platforms.join(', ')}`);
}
