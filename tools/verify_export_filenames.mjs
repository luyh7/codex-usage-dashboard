import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, mkdtemp, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { gunzipSync } from 'node:zlib';
import { cacheDir } from './runtime-paths.mjs';

const root = process.cwd();
const binary = path.join(root, 'target', 'debug', 'codex-usage-dashboard' + (process.platform === 'win32' ? '.exe' : ''));
const fixture = await mkdtemp(path.join(os.tmpdir(), 'cousash-export-names-'));
const home = path.join(fixture, 'home');
const state = path.join(fixture, 'state');
await mkdir(path.join(home, 'sessions'), { recursive: true });
await mkdir(state, { recursive: true });
const env = { ...process.env, COUSASH_CONFIG_DIR: state, CODEX_HOME: home, COMPUTERNAME: '研发 Mac/Desk' };
const child = spawn(binary, ['--serve', '--port', '0', '--no-auto-windows'], {
  cwd: root, env, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true,
});
let stdout = '', stderr = '';
child.stdout.on('data', chunk => stdout += chunk);
child.stderr.on('data', chunk => stderr += chunk);
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(fn) {
  for (let i = 0; i < 600; i++) {
    if (child.exitCode !== null) throw Error('Dashboard exited: ' + stderr);
    try { const value = await fn(); if (value) return value; } catch {}
    await delay(50);
  }
  throw Error('Dashboard did not become ready: ' + stderr);
}
function filename(response) {
  const header = response.headers.get('content-disposition') || '';
  const extended = header.match(/filename\*=UTF-8''([^;]+)/i)?.[1];
  return extended ? decodeURIComponent(extended) : header.match(/filename="([^"]+)"/)?.[1];
}
try {
  const base = await until(() => stdout.match(/http:\/\/127\.0\.0\.1:\d+/)?.[0]);
  const health = await until(async () => {
    const value = await (await fetch(base + '/api/health')).json();
    return value.index.phase === 'ready' && value;
  });
  const expected = `cud-研发-Mac-Desk-${health.device_short_code}.json.gz`;
  const exported = await fetch(base + '/api/export.json.gz');
  assert.equal(exported.status, 200);
  assert.equal(filename(exported), expected, 'browser export should identify its source device');
  const snapshot = JSON.parse(gunzipSync(Buffer.from(await exported.arrayBuffer())));
  assert.equal(snapshot.device.short_code, health.device_short_code);
  assert.equal(snapshot.device.label, '研发 Mac/Desk');

  const jsonExport = await fetch(base + '/api/export.json');
  assert.equal(filename(jsonExport), expected.slice(0, -3));
  const nativeName = await (await fetch(base + '/api/export/filename')).json();
  assert.equal(nativeName.filename, expected, 'desktop save dialog should use the same name');

  const token = (await (await fetch(base + '/api/security/token')).json()).token;
  const job = await (await fetch(base + '/api/export/start', {
    method: 'POST', headers: { 'Content-Type': 'application/json', 'x-codex-write-token': token }, body: '{}',
  })).json();
  await until(async () => (await (await fetch(base + '/api/export/progress?id=' + encodeURIComponent(job.id))).json()).phase === 'ready');
  assert.equal(filename(await fetch(base + '/api/export.json.gz?job=' + encodeURIComponent(job.id))), expected);

  const shared = await fetch(base + '/api/export.json.gz?mode=redacted');
  assert.equal(filename(shared), 'cud-shared.json.gz');
  assert.equal((await (await fetch(base + '/api/export/filename?mode=redacted')).json()).filename, 'cud-shared.json.gz');
  assert.equal((await fetch(base + '/api/export/filename?mode=invalid')).status, 400);

  const cli = spawnSync(binary, ['--export-snapshot', '--no-auto-windows'], { cwd: fixture, env, encoding: 'utf8' });
  assert.equal(cli.status, 0, cli.stderr);
  assert.equal(JSON.parse(gunzipSync(await readFile(path.join(fixture, expected)))).device.short_code, health.device_short_code);
  console.log('Export filenames: browser, desktop suggestion, CLI, Unicode, redaction, and jobs verified');
} finally {
  if (child.exitCode === null) {
    const exited = once(child, 'exit');
    child.kill();
    await exited;
  }
  if (process.platform !== 'win32') await rm(cacheDir(state), { recursive: true, force: true });
  await rm(fixture, { recursive: true, force: true });
}
