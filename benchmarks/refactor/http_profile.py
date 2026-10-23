"""Actual current-worktree HTTP tests on port 8765 with isolated synthetic data."""
import concurrent.futures
import cProfile
import io
import json
from pathlib import Path
import pstats
import tempfile
import time
import urllib.request
from urllib.parse import urlencode
from backend import HERE, WORK, d, dump, stats
from verify_dashboard import verify

def request(path,body=None):
    raw=json.dumps(body).encode() if body is not None else None
    start=time.perf_counter()
    with urllib.request.urlopen(urllib.request.Request('http://127.0.0.1:8765'+path,data=raw,headers={'Content-Type':'application/json'}),timeout=30) as r:
        data=r.read();status=r.status
    return {'ms':(time.perf_counter()-start)*1000,'bytes':len(data),'status':status,'body':json.loads(data) if data else None}

def main():
    report={'port':8765,'source':'current Rust production server','data':'synthetic 160 sessions, isolated config; no user imports modified','requests':{}}
    with tempfile.TemporaryDirectory(prefix='http-config-',dir=WORK) as config:
        try:
            verify(codex_home=WORK/'home',config_home=config)
            cold=request('/api/sessions?period=all')
            assert cold['body']['summary']['session_count']==160
            report['requests']['first_list']={k:v for k,v in cold.items() if k!='body'}
            token=cold['body']['snapshot_token'];uid=cold['body']['sessions'][0]['uid']
            for name,path in [('warm_list','/api/sessions?period=all'),('unchanged_204','/api/sessions?'+urlencode({'period':'all','snapshot_token':token})),
                              ('detail','/api/session?'+urlencode({'id':uid,'snapshot_token':token})),('daily','/api/daily-usage'),('export','/api/export.json')]:
                samples=[request(path) for _ in range(7)]
                assert all(r['status']==(204 if name=='unchanged_204' else 200) for r in samples)
                report['requests'][name]=dict(stats([r['ms'] for r in samples]),bytes=samples[-1]['bytes'])
            with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
                results=list(pool.map(lambda _:request('/api/sessions?period=all'),range(16)))
            assert all(r['body']['summary']['session_count']==160 for r in results)
            report['requests']['four_concurrent_readers']=stats([r['ms'] for r in results])
            snapshot=request('/api/export.json')['body'];snapshot['device']['short_code']='synthetic-remote-probe'
            imp=request('/api/remotes/import',{'snapshot':snapshot,'label':'Probe remote'})
            assert imp['body']['ok'];report['requests']['remote_import_ms']=imp['ms']
            assert request('/api/sessions?period=all')['body']['summary']['session_count']==320
            renamed=request('/api/remotes/rename',{'device_short_code':'synthetic-remote-probe','label':'Renamed probe'})
            assert renamed['body']['ok'];report['requests']['remote_rename_ms']=renamed['ms']
            deleted=request('/api/remotes/delete',{'device_short_code':'synthetic-remote-probe','confirm':True})
            assert deleted['body']['ok'];report['requests']['remote_delete_ms']=deleted['ms']
            assert request('/api/sessions?period=all')['body']['summary']['session_count']==160
            report['remote_roundtrip']='import -> merged list -> rename -> delete -> original list passed'
        finally:report['restored_dashboard']=verify()
    analyzer=d.CodexUsageAnalyzer(WORK/'home',resolve_project_info=False)
    profile=cProfile.Profile();profile.enable();analyzer.scan('all');profile.disable();analyzer.close()
    output=io.StringIO();pstats.Stats(profile,stream=output).strip_dirs().sort_stats('cumulative').print_stats(35)
    (HERE/'results/cold-profile.txt').write_text(output.getvalue())
    dump(HERE/'results/http.json',report)
    print(json.dumps(report,indent=2))

if __name__=='__main__':main()
