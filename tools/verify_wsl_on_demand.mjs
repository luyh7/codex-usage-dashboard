// Windows integration: stopped WSL sources must never block local indexing or
// wake a distribution through startup, settings, polling, or a forced refresh.
// Uses a nonexistent distribution and synthetic local logs only.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { mkdir, mkdtemp, writeFile, appendFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

assert.equal(process.platform, 'win32', 'Run this integration check on Windows');
const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const binary = path.resolve(process.argv[2] || path.join(root, 'target/debug/codex-usage-dashboard.exe'));
const output = path.join(root, 'output/verification');
await mkdir(output, { recursive: true });
const temporary = await mkdtemp(path.join(output, 'wsl-on-demand-'));
const home = path.join(temporary, 'codex');
await mkdir(path.join(home, 'sessions'), { recursive: true });
const log = path.join(home, 'sessions/rollout-synthetic.jsonl');
const event = (total, second) => JSON.stringify({ timestamp: `2026-11-01T00:00:0${second}Z`, type: 'event_msg', payload: { type: 'token_count', info: { total_token_usage: { input_tokens: total, total_tokens: total } } } });
await writeFile(log, JSON.stringify({ type: 'session_meta', payload: { id: 'wsl-on-demand-local', model: 'gpt-5', cwd: path.join(temporary, 'project') } }) + '\n' + event(12, 1) + '\n');
const distro = 'Cousash-Offline-Verification';
const homes = [home, ...[String.raw`\\wsl.localhost`, String.raw`\\wsl$`].map(host => path.join(host, distro, 'home', 'fixture', '.codex'))];
const shutdown = randomBytes(32).toString('hex');
const env = { ...process.env, COUSASH_CONFIG_DIR: path.join(temporary, 'config'), COUSASH_SHUTDOWN_TOKEN: shutdown };
delete env.COUSASH_OWNER_PID;
delete env.COUSASH_INSTANCE_ID;
const running = () => {
  const bytes = execFileSync(path.join(process.env.SystemRoot, 'System32/wsl.exe'), ['--list', '--running', '--quiet'], { windowsHide: true, timeout: 6000 });
  return bytes.toString(bytes.includes(0) ? 'utf16le' : 'utf8').replace(/^\uFEFF/, '').trim().split(/\r?\n/).map(s => s.trim()).filter(Boolean).sort();
};
const before = running();
assert(!before.some(name => name.toLowerCase() === distro.toLowerCase()));
let server, base, stdout = '', stderr = '';
const until = async test => {
  const deadline = Date.now() + 60000;
  while (!await test()) {
    if (server?.exitCode !== null) throw Error('Server exited: ' + stderr);
    if (Date.now() > deadline) throw Error('Timed out: ' + stderr);
    await new Promise(resolve => setTimeout(resolve, 100));
  }
};
const get = async endpoint => {
  const response = await fetch(base + endpoint, { signal: AbortSignal.timeout(10000) });
  assert(response.ok, `${endpoint}: ${response.status}`);
  return response.json();
};
const start = async () => {
  stdout = ''; stderr = '';
  server = spawn(binary, ['--serve', '--port', '0', '--no-browser', '--no-auto-windows', ...homes.flatMap(home => ['--codex-home', home])], { env, cwd: root, windowsHide: true });
  server.stdout.on('data', data => { stdout += data; });
  server.stderr.on('data', data => { stderr += data; });
  server.on('error', error => { stderr += error.message; });
  await until(() => !!stdout.match(/http:\/\/127\.0\.0\.1:\d+/));
  base = stdout.match(/http:\/\/127\.0\.0\.1:\d+/)[0];
  await until(async () => (await get('/api/health')).index.phase === 'ready');
};
const stop = async () => {
  if (!server || server.exitCode !== null) return;
  const response = await fetch(base + '/api/shutdown', { method: 'POST', headers: { Authorization: 'Bearer ' + shutdown } });
  assert(response.ok);
  await until(() => server.exitCode !== null);
};
const refresh = async () => {
  const { token } = await get('/api/security/token');
  const response = await fetch(base + '/api/refresh', { method: 'POST', headers: { 'Content-Type': 'application/json', 'x-codex-write-token': token }, body: '{}' });
  assert(response.ok);
};
try {
  await start();
  const health = await get('/api/health');
  assert.equal(health.build_id, JSON.parse(execFileSync(binary, ['--build-info'], { env, windowsHide: true, encoding: 'utf8' })).build_id);
  const sources = await get('/api/sources');
  assert.equal(sources.filter(source => source.status === 'paused').length, 2);
  assert.equal((await get('/api/summary?period=all')).summary.usage.total_tokens, 12);
  const page = await fetch(base + '/?page=settings');
  assert.equal(page.status, 200);
  assert.match(await page.text(), /\/assets\//);
  await refresh();
  await appendFile(log, event(24, 2) + '\n');
  await until(async () => (await get('/api/summary?period=all')).summary.usage.total_tokens === 24);
  await stop();
  await start();
  assert.equal((await get('/api/summary?period=all')).summary.session_count, 1);
  assert.equal((await get('/api/summary?period=all')).summary.usage.total_tokens, 24);
  assert.equal((await get('/api/sources')).filter(source => source.status === 'paused').length, 2);
  await refresh();
  // Cross the default 30-second network poll as well as local 2-second polls.
  await new Promise(resolve => setTimeout(resolve, 32000));
  assert.equal((await get('/api/health')).index.phase, 'ready');
  assert.deepEqual(running(), before, 'The integration check must not start another distribution');
  console.log(JSON.stringify({ ok: true, build_id: health.build_id, paused_sources: 2, local_tokens: 24, checks: ['startup', 'settings', 'forced refresh', 'local updates', 'restart', 'background network poll', 'unchanged running distributions'], output: temporary }, null, 2));
} finally {
  try { await stop(); } finally { if (server && server.exitCode === null) server.kill(); }
}
