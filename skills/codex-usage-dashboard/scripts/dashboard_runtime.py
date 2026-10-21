"""Verified native artifacts and fixed-port lifecycle for the compatibility opener.

No downloads happen at launch. Install a release manifest explicitly with
install_runtime.py, or use COUSASH_BACKEND=python to roll back.
"""
from __future__ import annotations
import hashlib
import json
import os
from pathlib import Path
import platform
import secrets
import re
import shlex
import signal
import socket
import subprocess
import sys
import tempfile
import time
from urllib.request import Request, urlopen

PORT = 8765

def platform_key():
    system = {'Linux':'linux','Darwin':'darwin','Windows':'windows'}.get(platform.system())
    arch = {'x86_64':'x64','AMD64':'x64','arm64':'arm64','aarch64':'arm64'}.get(platform.machine())
    return f'{system}-{arch}' if system and arch else None

def runtime_root():
    return Path(__file__).resolve().parent.parent / 'runtime'

def native_binary():
    manifest = runtime_root() / 'manifest.json'
    if not manifest.exists(): return None
    data = json.loads(manifest.read_text(encoding='utf-8'))
    entry = data.get('platforms',{}).get(platform_key())
    if not entry: return None
    name = entry['file']
    if Path(name).name != name: raise RuntimeError('Invalid native artifact filename')
    binary = runtime_root() / name
    if not binary.is_file(): raise RuntimeError(f'Native artifact missing: {binary}; reinstall it or use COUSASH_BACKEND=python')
    if hashlib.sha256(binary.read_bytes()).hexdigest() != entry['sha256']:
        raise RuntimeError('Native artifact checksum mismatch; reinstall it or use COUSASH_BACKEND=python')
    info = json.loads(subprocess.check_output([str(binary),'--build-info'],text=True,timeout=10))
    if info.get('build_id') != entry['build_id'] or info.get('version') != data.get('version'):
        raise RuntimeError('Native artifact build identity mismatch')
    return binary, info

def health(host):
    try:
        with urlopen(f'http://{host}:{PORT}/api/health',timeout=.4) as response:
            result=json.load(response)
        return result if isinstance(result,dict) else {}
    except (OSError,ValueError): return {}

def state_path():
    # Per-user record; no credentials are exposed in health or command lines.
    root = Path(os.environ.get('COUSASH_CONFIG_DIR') or (Path.home()/'.codex-usage-dashboard'))
    root.mkdir(parents=True,exist_ok=True)
    return root/'launcher-process.json'

def write_state(value):
    path=state_path()
    fd,name=tempfile.mkstemp(prefix='.launcher-',dir=path.parent)
    try:
        with os.fdopen(fd,'w',encoding='utf-8') as stream: json.dump(value,stream)
        os.replace(name,path)
    finally:
        Path(name).unlink(missing_ok=True)

def command_for_pid(pid):
    if sys.platform.startswith('linux'):
        try: return [v.decode() for v in Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0') if v]
        except OSError: return []
    if sys.platform == 'darwin':
        result=subprocess.run(['ps','-p',str(pid),'-o','command='],capture_output=True,text=True,timeout=3)
        try:return shlex.split(result.stdout.strip())
        except ValueError:return []
    return []

def release_owned(host, python_script):
    with socket.socket() as probe:
        probe.settimeout(.3)
        if probe.connect_ex((host,PORT)) != 0:return
    current=health(host)
    if current.get('app') != 'codex-usage-dashboard':
        raise RuntimeError('Port 8765 belongs to an unidentified service; it was not stopped.')
    pid=current.get('pid')
    # Releases before protocol health exposed a PID: inspect the actual listener
    # and require the exact installed Python script argument before signalling it.
    if pid is None and sys.platform.startswith('linux'):
        output=subprocess.check_output(['ss','-ltnp','sport = :8765'],text=True,timeout=3)
        pids=set(map(int,re.findall(r'pid=(\d+)',output)))
        if len(pids)==1:pid=pids.pop()
    elif pid is None and sys.platform=='darwin':
        output=subprocess.check_output(['lsof','-nP','-iTCP:8765','-sTCP:LISTEN','-t'],text=True,timeout=3)
        pids=set(int(line) for line in output.splitlines() if line.isdigit())
        if len(pids)==1:pid=pids.pop()
    if not isinstance(pid,int) or pid<=1 or pid==os.getpid():
        raise RuntimeError('Port 8765 process identity cannot be verified; stop that process before retrying.')
    stopped=False
    try: record=json.loads(state_path().read_text(encoding='utf-8'))
    except (OSError,ValueError):record={}
    if (record.get('pid')==pid and record.get('build_id')==current.get('build_id')
        and record.get('instance_id')==current.get('instance_id') and record.get('shutdown_token')):
        request=Request(f'http://{host}:{PORT}/api/shutdown',data=b'',method='POST',headers={'Authorization':'Bearer '+record['shutdown_token']})
        with urlopen(request,timeout=3) as response:stopped=response.status==202
    elif str(python_script.resolve()) in command_for_pid(pid):
        os.kill(pid,signal.SIGINT);stopped=True
    if not stopped:
        raise RuntimeError('Port 8765 is an unmanaged dashboard; stop it before replacing it. No process was killed.')
    deadline=time.monotonic()+15
    while time.monotonic()<deadline:
        with socket.socket() as probe:
            probe.settimeout(.2)
            if probe.connect_ex((host,PORT)) != 0:return
        time.sleep(.05)
    raise RuntimeError('The verified dashboard did not release port 8765; no force-kill was attempted.')

def launch_environment():
    return {**os.environ,'COUSASH_SHUTDOWN_TOKEN':secrets.token_hex(32),'COUSASH_INSTANCE_ID':secrets.token_hex(16)}
