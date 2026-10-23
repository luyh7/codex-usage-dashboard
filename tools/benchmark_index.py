"""Sequential synthetic index, persistent restore, append and remote-cache gates."""
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import statistics
import subprocess
import sys
import tempfile
import time
from compatibility import ROOT,load_dashboard

def worker(home,cache,operation):
    d=load_dashboard();home=Path(home);cache=Path(cache)
    if operation=='remote':
        start=time.perf_counter();store=d.RemoteSnapshotStore('local-fixture',home);sessions,details,sources=store.transformed_sessions();summary=d.CodexUsageAnalyzer.build_summary_static(sessions);elapsed=(time.perf_counter()-start)*1000
        print(json.dumps({'samples_ms':[elapsed],'peak_rss_kib':resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,'sessions':summary['session_count'],'tokens':summary['usage']['total_tokens']}));return
    analyzer=d.CodexUsageAnalyzer([d.CodexLogSource('fixture','Synthetic',home)],persistent_cache=d.PersistentParseCache(cache),resolve_project_info=False,parallel_workers=0)
    if operation=='inventory':
        samples=[]
        for i in range(17):
            start=time.perf_counter();files=analyzer.iter_session_files();elapsed=(time.perf_counter()-start)*1000
            if i>=2:samples.append(elapsed)
        print(json.dumps({'samples_ms':samples,'files':len(files),'peak_rss_kib':resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,'metrics':analyzer.cache_metrics}));analyzer.close();return
    # Include constructor/cache open in the same observable operation as Rust.
    analyzer.close();start=time.perf_counter();analyzer=d.CodexUsageAnalyzer([d.CodexLogSource('fixture','Synthetic',home)],persistent_cache=d.PersistentParseCache(cache),resolve_project_info=False,parallel_workers=0);initial=analyzer.scan('all',include_remotes=False);elapsed=(time.perf_counter()-start)*1000;samples=[]
    if operation in ['cold','restore']:samples=[elapsed]
    elif operation=='warm':
        for i in range(17):
            start=time.perf_counter();current=analyzer.scan('all',include_remotes=False);elapsed=(time.perf_counter()-start)*1000;assert current['snapshot_token']==initial['snapshot_token']
            if i>=2:samples.append(elapsed)
    elif operation=='append':
        path=home/'sessions/rollout-0000.jsonl';original=path.read_bytes();mtime=path.stat().st_mtime_ns
        for i in range(17):
            n=81+i;usage={'input_tokens':n*1000,'cached_input_tokens':n*600,'output_tokens':n*120,'reasoning_output_tokens':n*40,'total_tokens':n*1120};row={'timestamp':f'2026-10-01T00:02:{i:02}Z','type':'event_msg','payload':{'type':'token_count','info':{'total_token_usage':usage,'last_token_usage':{'input_tokens':1000,'cached_input_tokens':600,'output_tokens':120,'reasoning_output_tokens':40,'total_tokens':1120}}}}
            with path.open('a') as stream:stream.write(json.dumps(row,separators=(',',':'))+'\n')
            start=time.perf_counter();current=analyzer.scan('all',include_remotes=False);elapsed=(time.perf_counter()-start)*1000;assert current['summary']['usage']['total_tokens']==initial['summary']['usage']['total_tokens']+(i+1)*1120
            if i>=2:samples.append(elapsed)
        path.write_bytes(original);os.utime(path,ns=(mtime,mtime))
    print(json.dumps({'samples_ms':samples,'sessions':len(initial['sessions']),'tokens':initial['summary']['usage']['total_tokens'],'peak_rss_kib':resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,'metrics':analyzer.cache_metrics}));analyzer.close()

def stats(values):return {'p50_ms':statistics.median(values),'p95_ms':sorted(values)[int(len(values)*.95+.999)-1],'samples_ms':values}
def run(variant,home,cache,operation):
    cmd=[sys.executable,__file__,'worker',str(home),str(cache),operation] if variant=='python' else [str(ROOT/'target/release/examples/index_benchmark'),str(home),str(cache),operation]
    start=time.perf_counter();result=json.loads(subprocess.check_output(cmd,text=True));result['process_wall_ms']=(time.perf_counter()-start)*1000;return result

def generate(home):
    (home/'sessions').mkdir(parents=True)
    for f in range(160):
        rows=[{'timestamp':'2026-10-01T00:00:00Z','type':'session_meta','payload':{'id':f'fixture-{f:04}','model':'gpt-5','cwd':f'/synthetic/project-{f%20}'}}]
        for i in range(80):
            n=i+1;usage={'input_tokens':n*1000,'cached_input_tokens':n*600,'output_tokens':n*120,'reasoning_output_tokens':n*40,'total_tokens':n*1120};rows.append({'timestamp':(dt.datetime(2026,10,1,tzinfo=dt.UTC)+dt.timedelta(seconds=i)).isoformat(),'type':'event_msg','payload':{'type':'token_count','info':{'total_token_usage':usage,'last_token_usage':{'input_tokens':1000,'cached_input_tokens':600,'output_tokens':120,'reasoning_output_tokens':40,'total_tokens':1120}}}})
        (home/'sessions'/f'rollout-{f:04}.jsonl').write_text('\n'.join(json.dumps(row,separators=(',',':')) for row in rows)+'\n')

def main():
    os.environ['TZ']='UTC';time.tzset();subprocess.run(['cargo','build','--release','--example','index_benchmark'],cwd=ROOT,check=True)
    report={'source_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'python_sha256':hashlib.sha256(__import__('reference_dashboard').SOURCE.read_bytes()).hexdigest(),'rust_binary_sha256':hashlib.sha256((ROOT/'target/release/examples/index_benchmark').read_bytes()).hexdigest(),'platform':platform.platform(),'python':platform.python_version(),'rust':subprocess.check_output(['rustc','--version'],text=True).strip(),'samples':15,'warmups':2,'scale':{'files':160,'events_per_file':80,'remote_sessions':10000},'method':'Sequential isolated workers; synthetic data, warm OS filesystem cache. Cold/restore include index open, parsing/cache decoding, persistence and first full view; process wall time separately retained. Warm/append reuse a writer; append includes first updated view. Peak RSS includes whole worker. Rust pricing-only transaction has no legacy runtime equivalent and is reported separately.','results':{}}
    with tempfile.TemporaryDirectory(prefix='index-benchmark-') as temp:
        root=Path(temp);home=root/'home';generate(home)
        for operation in ['cold','restore','warm','append']:
            variants={}
            for variant in ['python','rust']:
                rows=[];cache=root/(variant+'-'+operation+'.sqlite3')
                if operation=='restore':run(variant,home,cache,'cold')
                count=17 if operation in ['cold','restore'] else 1
                for i in range(count):
                    path=cache if operation!='cold' else root/f'{variant}-cold-{i}.sqlite3'
                    result=run(variant,home,path,operation)
                    assert result['sessions']==160 and result['tokens']==160*80*1120
                    if count==1 or i>=2:rows.append(result)
                values=[t for row in rows for t in row['samples_ms']];variants[variant]={**stats(values),'peak_rss_kib':max(row['peak_rss_kib'] for row in rows),'workers':rows}
            report['results'][operation]=variants;print(operation,{v:(round(r['p50_ms'],3),r['peak_rss_kib']) for v,r in variants.items()},flush=True)
        reprice=run('rust',home,root/'reprice.sqlite3','reprice');report['results']['reprice_rust']={**stats(reprice['samples_ms']),**reprice}
        fixture=json.loads((ROOT/'benchmarks/refactor/.work/frontend-10000.json').read_text());remotes=root/'remotes';remotes.mkdir();(remotes/'fixture-remote.json').write_text(json.dumps({'schema':'cousash.remote-snapshot','version':1,'device':{'short_code':'fixture-remote','label':'Synthetic remote'},'imported_at':'2026-10-01T00:00:00Z','exported_at':'2026-10-01T00:00:00Z','snapshot':{**fixture,'generated_at':'2026-10-01T00:00:00Z','details_by_uid':{}}}))
        variants={}
        for variant in ['python','rust']:
            rows=[run(variant,remotes,root/'unused.sqlite3','remote') for _ in range(17)][2:];variants[variant]={**stats([r['samples_ms'][0] for r in rows]),'peak_rss_kib':max(r['peak_rss_kib'] for r in rows),'workers':rows};assert all(r['sessions']==10000 for r in rows)
        assert variants['python']['workers'][0]['tokens']==variants['rust']['workers'][0]['tokens'];report['results']['remote_restore_merge']=variants
        report['memory_gates']={op:{'ratio':result['rust']['peak_rss_kib']/result['python']['peak_rss_kib'],'passed':result['rust']['peak_rss_kib']<=result['python']['peak_rss_kib']*1.2} for op,result in report['results'].items() if isinstance(result,dict) and 'python' in result}
    output=ROOT/'benchmarks/implementation/results/index-performance.json';output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report['memory_gates'],indent=2))
if __name__=='__main__':
    if sys.argv[1:2]==['worker']:worker(*sys.argv[2:])
    else:main()
