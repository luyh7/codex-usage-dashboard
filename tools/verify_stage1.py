"""Replay real HTTP/browser checks on 8765 with isolated synthetic data."""
import datetime as dt
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import sys
import tempfile
import urllib.request
from urllib.parse import urlencode

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'benchmarks/refactor'))
from verify_dashboard import verify


def main():
    with tempfile.TemporaryDirectory(prefix='dashboard-stage1-') as temp:
        home = Path(temp) / 'home'
        sessions = home / 'sessions'
        sessions.mkdir(parents=True)
        rows = [{'timestamp': '2026-10-01T00:00:00Z', 'type': 'session_meta', 'payload': {'id': 'long-session', 'model': 'gpt-5', 'cwd': '/synthetic/long'}}]
        for i in range(10003):
            usage = {'input_tokens': (i+1)*10, 'output_tokens': (i+1)*2, 'total_tokens': (i+1)*12}
            rows.append({'timestamp': (dt.datetime(2026,10,1,tzinfo=dt.UTC)+dt.timedelta(seconds=i)).isoformat(), 'type':'event_msg','payload':{'type':'token_count','info':{'total_token_usage':usage,'last_token_usage':{'input_tokens':10,'output_tokens':2,'total_tokens':12}}}})
        (sessions / 'rollout-long.jsonl').write_text('\n'.join(json.dumps(row) for row in rows)+'\n')
        try:
            verify(codex_home=home, config_home=Path(temp)/'config')
            subprocess.run(['node', 'tools/verify_stage1_browser.mjs'], cwd=ROOT, check=True)
        finally:
            print(json.dumps({'restored_current_worktree': verify()}))


if __name__ == '__main__':
    main()
