import { test } from 'node:test';
import assert from 'node:assert/strict';
import { HttpDashboard } from './http';

test('each write obtains the current process token, including after a restart', async () => {
  let generation = 0;
  const methods: string[] = [];
  const data = new HttpDashboard('', (async (input, init) => {
    if (input === '/api/security/token') return Response.json({ token: `token-${++generation}` });
    methods.push(String(input));
    assert.equal(new Headers(init?.headers).get('X-Codex-Write-Token'), `token-${generation}`);
    return Response.json({ok:true});
  }) as typeof fetch);
  await data.renameRemote('one','New');
  await data.deleteRemote('one');
  assert.equal(generation,2);
  assert.deepEqual(methods,['/api/remotes/rename','/api/remotes/delete']);
});

test('a rejected token fetch never sends the mutation', async () => {
  const paths: string[] = [];
  const data = new HttpDashboard('', (async input => {
    paths.push(String(input));
    return Response.json({error:'untrusted Origin'}, {status:403});
  }) as typeof fetch);
  await assert.rejects(data.deleteRemote('one'), /untrusted Origin/);
  assert.deepEqual(paths,['/api/security/token']);
});
