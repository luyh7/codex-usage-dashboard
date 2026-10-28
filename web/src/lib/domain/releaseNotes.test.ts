import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { parseReleaseNotes } from './releaseNotes';

test('reads the bilingual 1.1.1 release without labeling existing features as new', async () => {
  const source = await readFile(new URL('../../../../docs/releases/v1.1.1.md', import.meta.url), 'utf8');
  const zh = parseReleaseNotes(source, 'zh');
  const en = parseReleaseNotes(source, 'en');
  assert.deepEqual(zh.items.map(item => item.kind), ['added', 'added', 'feature', 'feature']);
  assert.match(zh.items[0].text, /^GPT-6\.1 Terra/);
  assert.match(en.items[0].text, /^GPT-6\.1 Terra/);
  assert.match(en.items[1].text, /multiple Codex log folders/);
  assert.ok(zh.items.every(item => !item.text.includes('<br>')));
  assert.equal(zh.fallback, '');
});

test('recognizes the real 1.1.2 fix and selects the requested language', async () => {
  const source = await readFile(new URL('../../../../docs/releases/v1.1.2.md', import.meta.url), 'utf8');
  assert.deepEqual(parseReleaseNotes(source, 'zh').items, [{ kind: 'fixed', text: '实时刷新后加载更多项目时，可能出现“游标与查询不匹配”的错误。' }]);
  assert.match(parseReleaseNotes(source, 'en').items[0].text, /^a cursor mismatch error/);
});

test('retains unknown sections, ignores installer lists, and falls back for plain text', () => {
  const source = '## 下载 · Downloads\n- installer\n\n## 安全 · Security\n- 提升安全性。<br>Improve security.\n\n## 修复 · Fixes\n- 修复问题。<br>Fix a problem.';
  assert.deepEqual(parseReleaseNotes(source, 'zh').items, [
    { kind: 'improved', text: '提升安全性。' },
    { kind: 'fixed', text: '问题。' },
  ]);
  assert.deepEqual(parseReleaseNotes('A short plain-text release note.', 'en'), { items: [], fallback: 'A short plain-text release note.' });
  assert.deepEqual(parseReleaseNotes(null, 'zh'), { items: [], fallback: '' });
  assert.deepEqual(parseReleaseNotes('## Features\n- Fix a single-language issue.', 'en').items, [{ kind: 'fixed', text: 'a single-language issue.' }]);
  const unusual = '## 修复 · Fixes\n- 修复一个问题。<br>Fix a problem.\n\n升级前请先备份。';
  assert.deepEqual(parseReleaseNotes(unusual, 'zh'), { items: [], fallback: unusual });
  assert.deepEqual(parseReleaseNotes('## 安装修复 · Installation fixes\n- 修复安装失败。<br>Fix installation.', 'zh').items, [{ kind: 'fixed', text: '安装失败。' }]);
});
