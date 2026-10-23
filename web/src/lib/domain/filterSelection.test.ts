import { test } from 'node:test';
import assert from 'node:assert/strict';
import { filterValues, filterSelection } from './filterSelection.ts';

test('multi-select query values preserve legacy links, punctuation, duplicates and empty selection', () => {
  assert.deepEqual(filterValues('gpt-6.1-sol'), ['gpt-6.1-sol']);
  const values = ['local,work', 'remote "laptop"'];
  assert.deepEqual(filterValues(filterSelection(values)), values);
  assert.deepEqual(filterValues('["local","local",""]'), ['local']);
  assert.equal(filterSelection(['local', 'local']), 'local');
  assert.equal(filterSelection([]), '');
  assert.deepEqual(filterValues('[]'), []);
  assert.deepEqual(filterValues('[broken'), ['[broken']);
});
