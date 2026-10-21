"""Synthetic dual-implementation HTTP contract, using the fixed port only."""
import concurrent.futures
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'benchmarks/refactor'))
from verify_dashboard import verify
from reference_dashboard import SOURCE as SCRIPT
from compatibility import load_dashboard,compare
BINARY=ROOT/('target/release/codex-usage-dashboard' if os.environ.get('COUSASH_RELEASE')=='1' else 'target/debug/codex-usage-dashboard')
BASE='http://127.0.0.1:8765'

def request(path,body=None):
    req=urllib.request.Request(BASE+path,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(req,timeout=30) as response:
            raw=response.read();return response.status,json.loads(raw) if raw else None
    except urllib.error.HTTPError as error:
        return error.code,json.loads(error.read())

def stop_owned():
    import re
    try:
        status,h=request('/api/health')
        if h.get('instance_id'):
            subprocess.run([str(ROOT/'target/release/codex-usage-dashboard'),'--stop'],check=True)
            return
    except (OSError,ValueError): pass
    sockets=subprocess.check_output(['ss','-ltnp','sport = :8765'],text=True)
    for pid in set(map(int,re.findall(r'pid=(\d+)',sockets))):
        cmd=Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')
        if str(SCRIPT).encode() not in cmd and str(BINARY).encode() not in cmd and str(ROOT/'target/debug/codex-usage-dashboard').encode() not in cmd and str(ROOT/'target/release/codex-usage-dashboard').encode() not in cmd:raise RuntimeError(f'8765 owner is not this worktree: {pid}')
        os.kill(pid,signal.SIGINT)
    deadline=time.monotonic()+15
    while 'LISTEN' in subprocess.check_output(['ss','-ltn','sport = :8765'],text=True):
        if time.monotonic()>deadline:raise RuntimeError('owned dashboard did not release port')
        time.sleep(.05)

def event(total,second):
    usage={'input_tokens':total*10,'output_tokens':total*2,'total_tokens':total*12}
    return {'timestamp':(dt.datetime(2026,10,1,tzinfo=dt.UTC)+dt.timedelta(seconds=second)).isoformat(),'type':'event_msg','payload':{'type':'token_count','info':{'total_token_usage':usage,'last_token_usage':{'input_tokens':10,'output_tokens':2,'total_tokens':12}}}}

def main():
    subprocess.run(['cargo','build','--workspace',*(['--release'] if os.environ.get('COUSASH_RELEASE')=='1' else [])],cwd=ROOT,check=True,stdout=subprocess.DEVNULL)
    d=load_dashboard();checks=[];transcript={}
    with tempfile.TemporaryDirectory(prefix='rust-http-contract-') as tmp:
        root=Path(tmp);home=root/'home';(home/'sessions').mkdir(parents=True);config=root/'config'
        for i in range(100):(home/'plugins/irrelevant-cache'/str(i)).mkdir(parents=True)
        rows=[{'timestamp':'2026-10-01T00:00:00Z','type':'session_meta','payload':{'id':'long-session','model':'gpt-5','cwd':'/synthetic/long'}}]+[event(i+1,i) for i in range(10003)]
        path=home/'sessions/rollout-long.jsonl';path.write_text('\n'.join(json.dumps(row) for row in rows)+'\n')
        env={**os.environ,'COUSASH_CONFIG_DIR':str(config),'TZ':'UTC'}
        proc=None
        try:
            stop_owned()
            with (root/'server.log').open('wb') as log:
                proc=subprocess.Popen([str(BINARY),'--serve','--codex-home',str(home),'--no-auto-windows','--timezone','UTC'],cwd=ROOT,env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=log)
            deadline=time.monotonic()+30
            while True:
                if proc.poll() is not None:raise RuntimeError((root/'server.log').read_text())
                try:
                    status,health=request('/api/health')
                    if status==200 and health['index']['phase']=='ready':break
                    if health['index']['phase']=='error':raise RuntimeError(health)
                except OSError:pass
                if time.monotonic()>deadline:raise RuntimeError('server did not publish')
                time.sleep(.05)
            build=json.loads(subprocess.check_output([str(BINARY),'--build-info'],text=True))
            assert health['implementation']=='rust' and health['build_id']==build['build_id'];checks.append('build-health')
            watches=sum(sum(line.startswith('inotify') for line in info.read_text().splitlines()) for info in Path(f'/proc/{proc.pid}/fdinfo').iterdir())
            assert watches<=4, f'unrelated directories watched: {watches}'
            checks.append('watcher-excludes-plugin-cache-subtrees')
            analyzer=d.CodexUsageAnalyzer(d.codex_sources_from_homes([home]),resolve_project_info=False)
            expected=analyzer.scan('all',include_remotes=False)
            status,snapshot=request('/api/sessions?period=all');assert status==200
            for key in ['sessions','summary','daily_usage','codex_sources','codex_home']:compare(expected[key],snapshot[key])
            uid=snapshot['sessions'][0]['uid'];token=snapshot['snapshot_token'];params=urllib.parse.urlencode({'id':uid,'snapshot_token':token})
            status,detail=request('/api/session?'+params);assert status==200;compare(expected['details_by_uid'][uid],detail);checks.append('legacy-list-detail-golden')
            assert request('/api/sessions?period=all&snapshot_token='+token)[0]==204
            assert request('/api/session?id='+uid+'&snapshot_token=unpublished')[0]==409
            assert request('/api/v2/events?id='+uid)[0]==400
            status,meta=request('/api/v2/session?'+params);assert status==200 and 'timeline' not in meta
            status,chart=request('/api/v2/chart?'+params);assert status==200 and len(chart['timeline'])==10003
            events=[];cursor=''
            while True:
                status,page=request('/api/v2/events?'+params+'&cursor='+urllib.parse.quote(cursor));assert status==200;events.extend(page['events'])
                if not page['next_cursor']:break
                cursor=page['next_cursor']
            compare(sorted(detail['timeline'],key=lambda r:d.parse_timestamp(r['timestamp']),reverse=True),events);checks.append('10003-events-complete-chart-and-paging')
            for start,end in [('2026-10-01','2026-10-01'),('2026-09-01','2026-09-02')]:
                expected_range=analyzer.scan('custom',start,end,include_remotes=False)
                status,ranged=request('/api/sessions?'+urllib.parse.urlencode({'period':'custom','start':start,'end':end}));assert status==200
                for key in ['sessions','summary','daily_usage']:compare(expected_range[key],ranged[key])
            checks.append('date-boundaries')
            status,export=request('/api/export.json');assert status==200 and export['schema']==d.SNAPSHOT_SCHEMA and export['version']==1
            compare(expected['details_by_uid'],export['snapshot']['details_by_uid']);assert export['device']['short_code']==d.current_device_short_code();checks.append('export-v1-device-id')
            incoming=json.loads(json.dumps(export));incoming['device']['short_code']='fixture-remote';incoming['device']['label']='Synthetic remote'
            _,old_page=request('/api/v2/sessions?period=all')
            assert request('/api/remotes/import',{'snapshot':incoming})[1]['needs_label']
            status,imported=request('/api/remotes/import',{'snapshot':incoming,'label':'Synthetic remote'});assert status==200,imported
            _,retained=request('/api/v2/sessions?'+urllib.parse.urlencode({'revision':old_page['page']['revision']}));assert retained['remotes']==old_page['remotes'] and retained['codex_sources']==old_page['codex_sources'];checks.append('old-revision-sources-and-remotes-consistent')
            status,with_remote=request('/api/sessions?period=all');assert len(with_remote['sessions'])==2,with_remote
            assert sum(s['total_token_usage']['total_tokens'] for s in with_remote['sessions'])==detail['total_token_usage']['total_tokens']*2
            status,renamed=request('/api/remotes/rename',{'device_short_code':'fixture-remote','label':'Renamed'});assert status==200,renamed
            assert request('/api/remotes')[1]['remotes'][0]['label']=='Renamed'
            assert request('/api/remotes/delete',{'device_short_code':'fixture-remote'})[0]==400
            assert request('/api/remotes/delete',{'device_short_code':'fixture-remote','confirm':True})[0]==200
            assert len(request('/api/sessions?period=all')[1]['sessions'])==1;checks.append('remote-import-rename-delete-roundtrip')
            if os.environ.get('COUSASH_BROWSER')=='1':subprocess.run(['node','tools/verify_web.mjs'],cwd=ROOT,check=True)
            # Four readers keep one immutable revision while the writer appends.
            def reader(_):
                for _ in range(12):
                    status,current=request('/api/sessions?period=all');assert status==200
                    p=urllib.parse.urlencode({'id':uid,'snapshot_token':current['snapshot_token']})
                    status,data=request('/api/v2/session?'+p)
                    if status==409:continue
                    assert status==200 and data['total_token_usage']==next(s for s in current['sessions'] if s['uid']==uid)['total_token_usage']
                return True
            with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
                futures=[pool.submit(reader,i) for i in range(4)]
                with path.open('a') as file:file.write(json.dumps(event(10004,10004))+'\n')
                assert request('/api/remotes/import',{'snapshot':incoming,'label':'Concurrent remote'})[0]==200
                assert all(f.result() for f in futures)
            assert request('/api/remotes/delete',{'device_short_code':'fixture-remote','confirm':True})[0]==200
            deadline=time.monotonic()+6
            while request('/api/sessions?period=all')[1]['summary']['usage']['total_tokens']!=10004*12:
                if time.monotonic()>deadline:raise AssertionError('append not visible')
                time.sleep(.1)
            checks.append('four-clients-during-append-and-remote-import')
            assert request('/api/health')[1]['index']['metrics']['incremental_parses']>=1
            if os.environ.get('COUSASH_PERFORMANCE')=='1':
                fixture=json.loads((ROOT/'benchmarks/refactor/.work/frontend-10000.json').read_text())
                bulk={'schema':d.SNAPSHOT_SCHEMA,'version':1,'device':{'short_code':'bulk-fixture','label':'Bulk fixture'},'snapshot':{**fixture,'details_by_uid':{}}}
                assert request('/api/remotes/import',{'snapshot':bulk,'label':'Bulk fixture'})[0]==200
                subprocess.run(['node','tools/benchmark_browser.mjs'],cwd=ROOT,check=True)
            transcript={'health':health,'list_keys':list(snapshot),'detail_keys':list(detail),'export_keys':list(export),'checks':checks}
            analyzer.close()
        except BaseException:
            if (root/'server.log').exists():print((root/'server.log').read_text()[-6000:],file=sys.stderr)
            raise
        finally:
            if proc and proc.poll() is None:proc.send_signal(signal.SIGINT);proc.wait(timeout=15)
            print(json.dumps({'restored_current_worktree':verify()}))
    out=ROOT/'benchmarks/implementation/results/http-contract.json';out.parent.mkdir(parents=True,exist_ok=True);out.write_text(json.dumps(transcript,indent=2)+'\n');print(json.dumps({'checks':checks,'passed':True}))
if __name__=='__main__':main()
