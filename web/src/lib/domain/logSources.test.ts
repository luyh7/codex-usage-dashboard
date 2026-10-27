import test from 'node:test';
import assert from 'node:assert/strict';
import { displaySourcePath, sourcePathKey } from './logSources';

test('source paths account for Windows canonical paths and WSL aliases', () => {
  assert.equal(displaySourcePath('\\\\?\\C:\\Users\\A\\.codex'), 'C:\\Users\\A\\.codex');
  assert.equal(displaySourcePath('\\\\?\\UNC\\wsl.localhost\\Ubuntu\\home'), '\\\\wsl.localhost\\Ubuntu\\home');
  assert.equal(sourcePathKey('\\\\?\\C:\\Users\\A\\.codex'), sourcePathKey('c:/users/a/.codex/'));
  assert.equal(sourcePathKey('\\\\?\\UNC\\wsl.localhost\\Ubuntu\\home\\Alice\\.codex'), sourcePathKey('\\\\wsl$\\ubuntu\\home\\Alice\\.codex'));
  assert.notEqual(sourcePathKey('\\\\wsl$\\Ubuntu\\home\\Alice'), sourcePathKey('\\\\wsl$\\Ubuntu\\home\\alice'));
  assert.notEqual(sourcePathKey('/home/Alice/.codex'), sourcePathKey('/home/alice/.codex'));
});
