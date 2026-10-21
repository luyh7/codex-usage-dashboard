"""Restart only this worktree's dashboard on the contract's fixed port."""
import ast
import json
import os
from pathlib import Path
import signal
import subprocess
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'skills/codex-usage-dashboard/scripts/codex_usage_dashboard.py'

def verify(restart=True, codex_home=None, config_home=None):
    sockets = subprocess.check_output(['ss', '-ltnp', 'sport = :8765'], text=True)
    import re
    pids = set(map(int, re.findall(r'pid=(\d+)', sockets)))
    for pid in pids:
        cmd = Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')
        if str(SCRIPT).encode() not in cmd:
            raise RuntimeError(f'Port 8765 is owned by another process: {pid}')
        if restart:
            os.kill(pid, signal.SIGINT)
    if restart or not pids:
        deadline = time.monotonic() + 10
        while 'LISTEN' in subprocess.check_output(['ss', '-ltn', 'sport = :8765'], text=True):
            if time.monotonic() >= deadline:
                raise RuntimeError('Dashboard did not release 8765 gracefully')
            time.sleep(.1)
        log = ROOT / 'benchmarks/refactor/.work/dashboard.log'
        log.parent.mkdir(parents=True, exist_ok=True)
        with log.open('ab') as out:
            command=['python3', '-u', str(SCRIPT), '--host', '127.0.0.1', '--port', '8765']
            if codex_home: command += ['--codex-home', str(codex_home), '--no-auto-windows']
            env=os.environ.copy()
            if config_home: env['COUSASH_CONFIG_DIR']=str(config_home)
            subprocess.Popen(command,env=env,cwd=ROOT, stdin=subprocess.DEVNULL, stdout=out, stderr=out, start_new_session=True)
    deadline = time.monotonic() + 15
    while True:
        try:
            with urllib.request.urlopen('http://127.0.0.1:8765/api/health', timeout=2) as r:
                health = json.load(r)
            break
        except OSError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(.1)
    constants = {}
    for node in ast.parse(SCRIPT.read_text()).body:
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id in ('HTML', 'DASHBOARD_FEATURES'):
                    constants[target.id] = ast.literal_eval(node.value)
    with urllib.request.urlopen('http://127.0.0.1:8765/', timeout=3) as r:
        html = r.read().decode()
    assert health['ok'] and health['features'] == constants['DASHBOARD_FEATURES']
    assert html == constants['HTML']
    return {'health_ok': True, 'worktree_html_match': True, 'worktree_features_match': True}

if __name__ == '__main__':
    import sys
    print(json.dumps(verify('--check' not in sys.argv)))
