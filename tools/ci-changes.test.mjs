import test from 'node:test';
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {mkdtemp, mkdir, readFile, writeFile, rename, rm} from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {planChanges, isDocumentation} from './ci-changes.mjs';

async function repository(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'cousash-ci-'));
  t.after(() => { assert(root.startsWith(path.join(os.tmpdir(), 'cousash-ci-'))); return rm(root, {recursive: true, force: true}); });
  const git = args => execFileSync('git', args, {cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe']}).trim();
  git(['init', '-q', '-b', 'main']); git(['config', 'core.autocrlf', 'false']);
  async function commit(file, content) {
    if (file) { await mkdir(path.dirname(path.join(root, file)), {recursive: true}); await writeFile(path.join(root, file), content); }
    git(['add', '-A']);
    git(['-c', 'user.name=CI test', '-c', 'user.email=ci@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '-qm', 'fixture']);
    return git(['rev-parse', 'HEAD']);
  }
  const base = await commit('README.md', 'initial\n');
  return {root, git, commit, base, plan: (name, event) => planChanges(name, event, {cwd: root, fetchMissing: false})};
}
const pr = (base, head, before, action = 'synchronize') => ({action, before, pull_request: {base: {sha: base}, head: {sha: head}}});

test('a docs push into a code PR skips rebuild, while opening that PR validates its code', async t => {
  const r = await repository(t), code = await r.commit('crates/usage-core/src/example.rs', 'fn main() {}\n');
  const docs = await r.commit('docs/CI 使用.md', 'guide\n');
  assert.equal(r.plan('pull_request', pr(r.base, docs, code)).rebuild, false);
  assert.equal(r.plan('pull_request', pr(r.base, docs, undefined, 'opened')).rebuild, true);
  assert.equal(r.plan('pull_request', pr(r.base, docs, undefined, 'reopened')).rebuild, true);
});
test('a multi-commit push checks the whole pushed range, not only its final docs commit', async t => {
  const r = await repository(t); await r.commit('web/src/new.ts', 'export const value = 1;\n');
  const head = await r.commit('docs/note.md', 'note\n');
  assert.equal(r.plan('pull_request', pr(r.base, head, r.base)).rebuild, true);
});
test('an unavailable pre-force-push revision falls back to the full PR diff', async t => {
  const r = await repository(t), head = await r.commit('desktop/src/new.rs', '// source\n');
  assert.equal(r.plan('pull_request', pr(r.base, head, '1'.repeat(40))).rebuild, true);
  assert.equal(r.plan('pull_request', pr(r.base, head, '--help')).rebuild, true);
  assert.equal(r.plan('pull_request', {action: 'synchronize'}).rebuild, true);
});
test('force-push compares the before and after trees even when they are not ancestors', async t => {
  const r = await repository(t), before = await r.commit('crates/new.rs', '// first\n');
  r.git(['checkout', '-qb', 'replacement', r.base]);
  const after = await r.commit('README.md', 'new docs\n');
  assert.equal(r.plan('pull_request', pr(r.base, after, before)).rebuild, true);
});
test('moving source into docs still validates the removed source path', async t => {
  const r = await repository(t), before = await r.commit('tools/check.mjs', 'export {};\n');
  await mkdir(path.join(r.root, 'docs')); await rename(path.join(r.root, 'tools/check.mjs'), path.join(r.root, 'docs/check.md'));
  const after = await r.commit();
  assert.equal(r.plan('pull_request', pr(r.base, after, before)).rebuild, true);
});
test('main docs pushes skip; source, new branches and missing history validate', async t => {
  const r = await repository(t), docs = await r.commit('README.md', 'updated\n');
  assert.equal(r.plan('push', {before: r.base, after: docs, ref: 'refs/heads/main'}).rebuild, false);
  const source = await r.commit('unknown-new-build-input', 'build\n');
  assert.equal(r.plan('push', {before: docs, after: source}).rebuild, true);
  assert.equal(r.plan('push', {before: '0'.repeat(40), after: source}).rebuild, true);
  assert.equal(r.plan('push', {before: '1'.repeat(40), after: source}).rebuild, true);
});
test('manual and release events always validate, even for docs-only content', () => {
  assert.equal(planChanges('workflow_dispatch', {}).rebuild, true);
  assert.equal(planChanges('push', {ref: 'refs/tags/v1.0.1'}).rebuild, true);
  assert.equal(planChanges('merge_group', {}).rebuild, true);
});
test('empty ranges skip; non-doc fixture files and workflow changes stay covered', async t => {
  const r = await repository(t);
  assert.equal(r.plan('pull_request', pr(r.base, r.base, r.base)).rebuild, false);
  for (const file of ['.github/workflows/native.yml', 'tools/ci-changes.mjs', 'fixtures/log.txt', 'web/package-lock.json', 'desktop/icons/icon.png']) assert.equal(isDocumentation(file), false);
  for (const file of ['docs/image.png', 'README.md', 'AGENTS.md', 'skills/example/SKILL.md', 'LICENSE']) assert.equal(isDocumentation(file), true);
});
test('the CLI emits the GitHub output that actually gates expensive validation', async t => {
  const r = await repository(t), code = await r.commit('crates/new.rs', '// code\n'), docs = await r.commit('docs/note.md', 'note\n');
  const event = path.join(r.root, '事件.json'), output = path.join(r.root, 'output'), summary = path.join(r.root, 'summary');
  await writeFile(event, JSON.stringify(pr(r.base, docs, code)));
  const result = JSON.parse(execFileSync(process.execPath, [fileURLToPath(new URL('./ci-changes.mjs', import.meta.url))], {cwd: r.root, encoding: 'utf8', env: {...process.env, GITHUB_EVENT_NAME: 'pull_request', GITHUB_EVENT_PATH: event, GITHUB_OUTPUT: output, GITHUB_STEP_SUMMARY: summary}}));
  assert.equal(result.rebuild, false);
  assert.equal(await readFile(output, 'utf8'), 'rebuild=false\n');
  assert.match(await readFile(summary, 'utf8'), /Keep existing code validation running/);
});
