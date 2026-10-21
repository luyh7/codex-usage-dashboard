"""Developer verification: run the current Rust worktree on fixed port 8765."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import urllib.request
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools'))
BINARY = ROOT / 'target/release/codex-usage-dashboard'
_last_env = None

def verify(restart=True, codex_home=None, config_home=None):
    global _last_env
    env = os.environ.copy()
    if config_home: env['COUSASH_CONFIG_DIR'] = str(config_home)
    if restart:
        # Only authenticated managed instances can be stopped by this command.
        subprocess.run([str(BINARY), '--stop'], env=_last_env, check=True)
    command = [str(BINARY), '--no-browser']
    if codex_home: command += ['--codex-home', str(codex_home), '--no-auto-windows']
    subprocess.run(command, env=env, check=True, stdout=subprocess.DEVNULL)
    _last_env = env
    expected = json.loads(subprocess.check_output([str(BINARY), '--build-info']))
    deadline = time.monotonic() + 60
    while True:
        with urllib.request.urlopen('http://127.0.0.1:8765/api/health', timeout=3) as response:
            health = json.load(response)
        if health['index']['phase'] == 'ready': break
        if health['index']['phase'] == 'error': raise RuntimeError(health['index']['error'])
        if time.monotonic() > deadline: raise RuntimeError('Index readiness timed out')
        time.sleep(.1)
    with urllib.request.urlopen('http://127.0.0.1:8765/', timeout=3) as response:
        html = response.read()
    assert health['implementation'] == 'rust' and health['build_id'] == expected['build_id']
    assert html == (ROOT / 'web/classic/index.html').read_bytes()
    return {'health_ok': True, 'worktree_html_match': True, 'build_id': health['build_id']}

if __name__ == '__main__': print(json.dumps(verify('--check' not in sys.argv)))
