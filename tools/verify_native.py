"""Portable, synthetic fixed-port release/installer/lifecycle acceptance.

Requires a packaged local release. Refuses an occupied 8765 unless
--replace-worktree is explicitly supplied on Linux for the local verifier.
"""
import argparse
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
from urllib.error import HTTPError
from urllib.request import Request,urlopen
ROOT=Path(__file__).resolve().parents[1]
BASE='http://127.0.0.1:8765'

def request(path,token=None):
    req=Request(BASE+path,data=b'' if token else None,headers={'Authorization':'Bearer '+token} if token else {})
    with urlopen(req,timeout=3) as response:
        body=response.read();return response.status,json.loads(body) if body else None

def free():
    with socket.socket() as sock:return sock.connect_ex(('127.0.0.1',8765))!=0

def wait_free():
    deadline=time.monotonic()+15
    while not free():
        if time.monotonic()>deadline:raise AssertionError('port not released gracefully')
        time.sleep(.05)

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--replace-worktree',action='store_true');args=parser.parse_args()
    if args.replace_worktree:
        from verify_server import stop_owned
        stop_owned()
    elif not free():raise RuntimeError('8765 already occupied; no process was stopped')
    checks=[];report={'platform':platform.platform(),'python':platform.python_version(),'checks':checks,'native':platform.system()}
    with tempfile.TemporaryDirectory(prefix='native-acceptance-') as temp:
        root=Path(temp);codex=root/'codex';(codex/'sessions').mkdir(parents=True);config=root/'config';env={**os.environ,'CODEX_HOME':str(codex),'COUSASH_CONFIG_DIR':str(config),'COUSASH_DASHBOARD_LAUNCH_LOG':str(root/'launch.log'),'TZ':'UTC'}
        skill=codex/'skills/codex-usage-dashboard';scripts=skill/'scripts';manifest=ROOT/'dist/release/manifest.json'
        def run(*cmd,ok=True):
            result=subprocess.run(list(map(str,cmd)),env=env,cwd=root,capture_output=True,text=True,timeout=45)
            if ok and result.returncode:raise AssertionError(result.stdout+result.stderr)
            return result
        def opener(*extra,ok=True):return run(sys.executable,scripts/'open_dashboard.py','--no-browser','--codex-home',codex,'--no-auto-windows',*extra,ok=ok)
        try:
            run('node',ROOT/'bin/install.js');assert (scripts/'codex_usage_dashboard.py').is_file();checks.append('first npm install')
            assert opener('--backend','rust',ok=False).returncode!=0;checks.append('missing binary has actionable failure')
            run(sys.executable,scripts/'install_runtime.py',manifest,'--offline');checks.append('offline verified native installation')
            installed=skill/'runtime/manifest.json';before=installed.read_bytes();data=json.loads(before);entry=next(iter(data['platforms'].values()));binary=skill/'runtime'/entry['file'];report['build_id']=entry['build_id'];report['binary_sha256']=entry['sha256']
            # Failed upgrades preserve the previous installation.
            bad=root/'bad';bad.mkdir();bad_data=json.loads(before);bad_entry=next(iter(bad_data['platforms'].values()));bad_entry['sha256']='0'*64;(bad/'manifest.json').write_text(json.dumps(bad_data));shutil.copyfile(binary,bad/entry['file'])
            assert run(sys.executable,scripts/'install_runtime.py',bad/'manifest.json','--offline',ok=False).returncode!=0
            assert installed.read_bytes()==before
            (bad/entry['file']).unlink();assert run(sys.executable,scripts/'install_runtime.py',bad/'manifest.json','--offline',ok=False).returncode!=0
            assert installed.read_bytes()==before;checks.append('checksum failure and missing offline artifact preserve installation')
            # Existing external config and archives survive forced npm upgrade.
            (config/'remotes').mkdir(parents=True);sentinel=config/'remotes/preserve.txt';sentinel.write_text('synthetic user data')
            run('node',ROOT/'bin/install.js','--force');assert sentinel.read_text()=='synthetic user data' and installed.read_bytes()==before;checks.append('forced upgrade preserves config, snapshots and native rollback artifacts')
            run(sys.executable,scripts/'install_runtime.py',manifest,'--offline');run(sys.executable,scripts/'install_runtime.py','--rollback');assert installed.read_bytes()==before;checks.append('native manifest rollback')
            # Foreign listener cannot pass app/process identification.
            class Foreign(http.server.BaseHTTPRequestHandler):
                def do_GET(self):self.send_response(200);self.end_headers();self.wfile.write(b'{"app":"unrelated"}')
                def log_message(self,*args):pass
            server=http.server.HTTPServer(('127.0.0.1',8765),Foreign);thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
            try:assert opener('--backend','rust',ok=False).returncode!=0 and thread.is_alive()
            finally:server.shutdown();server.server_close();thread.join()
            checks.append('unknown port owner left running')
            rows=[{'type':'session_meta','payload':{'id':'native-fixture','model':'gpt-5','cwd':str(root/'project')}},{'timestamp':'2026-10-01T00:00:00Z','type':'event_msg','payload':{'type':'token_count','info':{'total_token_usage':{'input_tokens':10,'output_tokens':2,'total_tokens':12}}}}]
            (codex/'sessions/rollout-native.jsonl').write_text('\n'.join(json.dumps(r) for r in rows)+'\n',encoding='utf-8')
            once=run(binary,'--codex-home',codex,'--no-auto-windows','--once','--json');assert json.loads(once.stdout)['summary']['usage']['total_tokens']==12;checks.append('native CLI once')
            export=root/'export.json';run(binary,'--codex-home',codex,'--no-auto-windows','--export-snapshot',export);assert json.loads(export.read_text())['version']==1;checks.append('native CLI export v1')
            if os.name!='nt':
                old_script=scripts/'codex_usage_dashboard.py';current_script=old_script.read_bytes()
                old_script.write_bytes(subprocess.check_output(['git','show','c42f0e8ae948d4cde392fafa9894681a1200b17d:skills/codex-usage-dashboard/scripts/codex_usage_dashboard.py'],cwd=ROOT))
                legacy=subprocess.Popen([sys.executable,str(old_script),'--codex-home',str(codex),'--no-auto-windows'],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
                try:
                    deadline=time.monotonic()+15
                    while True:
                        try:
                            if request('/api/health')[0]==200:break
                        except OSError:pass
                        if time.monotonic()>deadline:raise AssertionError('old install did not start')
                        time.sleep(.05)
                    old_script.write_bytes(current_script)
                    opener('--backend','rust');legacy.wait(timeout=15);checks.append('upgrade from frozen old Python installation without health PID')
                finally:
                    old_script.write_bytes(current_script)
                    if legacy.poll() is None:legacy.send_signal(__import__('signal').SIGINT);legacy.wait(timeout=15)
            opener('--backend','rust');health=request('/api/health')[1];assert health['build_id']==entry['build_id'] and health['ui']=='legacy';pid=health['pid'];opener('--backend','rust');assert request('/api/health')[1]['pid']==pid;checks.append('verified native launch and same-build reuse')
            incoming=json.loads(export.read_text());incoming['device']['short_code']='once-fixture'
            with urlopen(Request(BASE+'/api/remotes/import',data=json.dumps({'snapshot':incoming,'label':'Once fixture'}).encode(),headers={'Content-Type':'application/json'}),timeout=5) as response:assert response.status==200
            once=run(binary,'--codex-home',codex,'--no-auto-windows','--once','--json');assert json.loads(once.stdout)['summary']['usage']['total_tokens']==24
            run(binary,'--codex-home',codex,'--no-auto-windows','--export-snapshot','relative-export.json');assert json.loads((root/'relative-export.json').read_text())['snapshot']['summary']['usage']['total_tokens']==12
            assert request('/api/health')[1]['pid']==pid;checks.append('one-shot CLI coexists with server, includes remotes; relative export remains local-only')
            # An authenticated stop must also close an active SSE subscription.
            stream=urlopen(BASE+'/api/revisions',timeout=3);record=json.loads((config/'launcher-process.json').read_text());assert request('/api/shutdown',record['shutdown_token'])[0]==202;wait_free();stream.close();checks.append('authenticated graceful shutdown with SSE and port release')
            opener('--backend','rust');opener('--backend','python');assert request('/api/health')[1].get('implementation')!='rust';checks.append('explicit Python rollback replaces managed Rust gracefully')
            if os.name!='nt':
                opener('--backend','rust');assert request('/api/health')[1]['implementation']=='rust';checks.append('verified Python process replaced by Rust')
        finally:
            if not free():
                try:
                    health=request('/api/health')[1]
                    if health.get('implementation')=='rust':
                        record=json.loads((config/'launcher-process.json').read_text());request('/api/shutdown',record['shutdown_token']);wait_free()
                    elif health.get('pid'):
                        if os.name=='nt':subprocess.run(['taskkill','/PID',str(health['pid']),'/T','/F'],capture_output=True)
                        else:os.kill(health['pid'],2)
                        wait_free()
                except Exception as error:print(f'cleanup: {error}',file=sys.stderr)
            if args.replace_worktree:
                from verify_dashboard import verify
                verify()
    report['passed']=True;out=ROOT/'benchmarks/implementation/results'/f'native-{platform.system().lower()}-{platform.machine()}.json';out.parent.mkdir(parents=True,exist_ok=True);out.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report))
if __name__=='__main__':main()
