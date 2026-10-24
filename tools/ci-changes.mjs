// PR path filters compare the whole PR, so inspect the latest push separately.
// Failure to establish that range must request full validation, never skip it.
import {execFileSync} from 'node:child_process';
import {appendFileSync, readFileSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const revision = value => typeof value === 'string' && /^[a-f0-9]{40}(?:[a-f0-9]{24})?$/i.test(value) && !/^0+$/.test(value);
export const isDocumentation = file => file.startsWith('docs/') || /\.(md|rst)$/i.test(file) || ['LICENSE', 'LICENSE.txt', 'NOTICE', 'NOTICE.txt'].includes(file);

export function planChanges(eventName, event, {cwd = process.cwd(), fetchMissing = true} = {}) {
  const full = reason => ({rebuild: true, reason});
  if (eventName === 'workflow_dispatch' || event.ref?.startsWith('refs/tags/')) return full('manual or release validation');
  const git = args => execFileSync('git', args, {cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe']}).trim();
  function ensure(value) {
    if (!revision(value)) throw Error('missing revision');
    try { git(['cat-file', '-e', value + '^{commit}']); }
    catch {
      if (!fetchMissing) throw Error('unavailable revision');
      git(['fetch', '--no-tags', 'origin', value]);
      git(['cat-file', '-e', value + '^{commit}']);
    }
    return value;
  }
  function diff(before, after, mergeBase = false) {
    ensure(before); ensure(after);
    const base = mergeBase ? git(['merge-base', before, after]) : before;
    // Report both halves of a rename, including code moved into docs/.
    const output = execFileSync('git', ['diff', '--no-renames', '--name-only', '-z', base, after, '--'], {cwd, encoding: 'utf8'});
    const files = output.split('\0').filter(Boolean);
    return {rebuild: files.some(file => !isDocumentation(file)), reason: files.some(file => !isDocumentation(file)) ? 'build inputs changed' : 'documentation only or empty diff', changed_files: files.length, before: base, after};
  }
  if (eventName === 'pull_request') {
    if (event.action === 'synchronize') {
      try { return diff(event.before, event.pull_request?.head?.sha); }
      catch { /* Force-pushed or unavailable history: inspect the complete PR. */ }
    }
    try { return diff(event.pull_request?.base?.sha, event.pull_request?.head?.sha, true); }
    catch { return full('PR history unavailable'); }
  }
  if (eventName === 'push') {
    try { return diff(event.before, event.after); }
    catch { return full('push history unavailable'); }
  }
  return full('event requires full validation');
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = planChanges(process.env.GITHUB_EVENT_NAME, JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8')));
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `rebuild=${result.rebuild}\n`);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `CI: ${result.reason}. ${result.rebuild ? 'Run shared checks and all four native/desktop targets.' : 'Keep existing code validation running; no rebuild requested.'}\n`);
  console.log(JSON.stringify(result));
}
