"""Same-machine synthetic module measurements, each worker in its own process."""
import datetime as dt
import json
from pathlib import Path
import platform
import resource
import statistics
import subprocess
import sys
import tempfile
import time
from compatibility import ROOT,load_dashboard


def worker(path,operation):
    d=load_dashboard();data=json.loads(Path(path).read_text());pricing=d
    analyzer=d.CodexUsageAnalyzer([d.CodexLogSource('fixture','Synthetic',Path(data['home']))],resolve_project_info=False)
    functions={
        'timeline':lambda:d.timeline_rows_with_deltas(data['timeline']),
        'pricing':lambda:d.pricing_for_timeline(data['timeline'],'gpt-5'),
        'lookup':lambda:[d.price_entry_for_model(q['model'],q['timestamp'],q['input_tokens'],q['service_tier']) for q in data['quotes']],
        'reader':lambda:list(d.read_rollout_jsonl(Path(data['path']))),
        'parse_file':lambda:analyzer.parse_file(Path(data['path']),'active'),
    }
    # Match Rust's resident normalized events; both workers load the same input.
    normalized=d.timeline_rows_with_deltas(data['timeline'])
    samples=[]
    for i in range(17):
        started=time.perf_counter();result=functions[operation]();elapsed=(time.perf_counter()-started)*1000
        if i>=2:samples.append(elapsed)
        del result
    print(json.dumps({'samples_ms':samples,'peak_rss_kib':resource.getrusage(resource.RUSAGE_SELF).ru_maxrss}))
    analyzer.close()


def main():
    report={'samples':15,'warmups':2,'scale':10000,'platform':platform.platform(),'python':platform.python_version(),'rust':subprocess.check_output(['rustc','--version'],text=True).strip(),'method':'Sequential isolated workers, warm OS cache, no parse cache; excludes startup/input decoding/output encoding/destruction. Peak RSS is entire worker including input and resident normalized timeline, not isolated operation allocations. Rust pricing reuses normalized events; Python pricing includes normalization, so that row measures architectural reuse as well as implementation.','results':{}}
    with tempfile.TemporaryDirectory(prefix='core-benchmark-') as temp:
        home=Path(temp);path=home/'rollout-benchmark.jsonl';input_path=home/'input.json'
        events=[];rows=[{'timestamp':'2026-10-01T00:00:00Z','type':'session_meta','payload':{'id':'synthetic-benchmark','model':'gpt-5','cwd':'/synthetic/project'}}]
        for i in range(10000):
            timestamp=(dt.datetime(2026,10,1,tzinfo=dt.UTC)+dt.timedelta(seconds=i)).isoformat()
            usage={'input_tokens':(i+1)*1000,'cached_input_tokens':(i+1)*600,'output_tokens':(i+1)*120,'reasoning_output_tokens':(i+1)*40,'total_tokens':(i+1)*1120}
            last={'input_tokens':1000,'cached_input_tokens':600,'output_tokens':120,'reasoning_output_tokens':40,'total_tokens':1120}
            events.append({'timestamp':timestamp,'model':'gpt-5','total_token_usage':usage,'last_token_usage':last})
            rows.append({'timestamp':timestamp,'type':'event_msg','payload':{'type':'token_count','info':{'total_token_usage':usage,'last_token_usage':last}}})
        path.write_text('\n'.join(json.dumps(row,separators=(',',':')) for row in rows)+'\n')
        quotes=[{'model':['gpt-5','gpt-6.1-sol','gpt-5.6-sol'][i%3],'timestamp':events[i]['timestamp'],'input_tokens':i*100,'service_tier':['','priority','ultrafast'][i%3]} for i in range(10000)]
        input_path.write_text(json.dumps({'path':str(path),'home':str(home),'timeline':events,'quotes':quotes}))
        report['jsonl_bytes']=path.stat().st_size
        for operation in ['timeline','pricing','lookup','reader','parse_file']:
            results={}
            for variant in ['python','rust']:
                command=[sys.executable,__file__,'worker',str(input_path),operation] if variant=='python' else [str(ROOT/'target/release/examples/benchmark'),str(input_path),operation]
                result=json.loads(subprocess.check_output(command,text=True));ordered=sorted(result['samples_ms']);result.update(p50_ms=statistics.median(ordered),p95_ms=ordered[-1]);results[variant]=result
            report['results'][operation]=results
    (ROOT/'benchmarks/implementation/results/core-performance.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:{v:{f:x[f] for f in ['p50_ms','p95_ms','peak_rss_kib']} for v,x in row.items()} for k,row in report['results'].items()},indent=2))


if __name__=='__main__':
    if sys.argv[1:2]==['worker']:worker(sys.argv[2],sys.argv[3])
    else:main()
