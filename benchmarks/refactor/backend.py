"""Reproducible synthetic module probes. Never reads user session content.

python3 benchmarks/refactor/backend.py generate
python3 benchmarks/refactor/backend.py run
"""
import argparse
import datetime as dt
import hashlib
import importlib.util
import itertools
import json
import math
import os
from pathlib import Path
import platform
import random
import resource
import sqlite3
import statistics
import subprocess
import sys
import time
import zlib

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
WORK = HERE / '.work'
SOURCE = ROOT / 'skills/codex-usage-dashboard/scripts/codex_usage_dashboard.py'
os.environ['TZ'] = 'Asia/Shanghai'
time.tzset()
spec = importlib.util.spec_from_file_location('dashboard_probe_source', SOURCE)
d = importlib.util.module_from_spec(spec)
spec.loader.exec_module(d)

def dump(path, value):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(value, ensure_ascii=False, separators=(',', ':')))

def u(i):
    return dict(zip(d.TOKEN_KEYS, [i*1000, i*600, 0, i*120, i*40, i*1120]))

def timestamp(i):
    return d.utc_iso(dt.datetime(2026, 10, 1, tzinfo=dt.UTC) + dt.timedelta(minutes=i))

def timeline(count, reset=False):
    return [{'timestamp':timestamp(i), 'model':'gpt-5', 'service_tier':'',
             'total_token_usage':u(i%97+1 if reset else i+1), 'last_token_usage':u(1)} for i in range(count)]

def session(i):
    root = i - i%4
    return {'uid':f'uid-{i:06d}', 'session_id':f'session-{i:06d}', 'root_session_id':f'session-{root:06d}',
            'parent_thread_id':f'session-{root:06d}' if i%4 else '', 'is_subagent':bool(i%4),
            'agent_path':f'/root/agent-{i%4}' if i%4 else '', 'agent_nickname':f'Agent {i%4}',
            'fork_usage_resolved':True, 'title':f'测试会话 {i:06d} — benchmark task',
            'model':['gpt-5','gpt-5.6-sol','gpt-6.1-sol'][i%3], 'models':[['gpt-5','gpt-5.6-sol','gpt-6.1-sol'][i%3]],
            'effort':'high', 'source':'archived' if i%7==0 else 'active',
            'project':f'project-{(i//4)%20:02d}', 'project_root':f'/synthetic/project-{(i//4)%20:02d}',
            'cwd':f'/synthetic/project-{(i//4)%20:02d}', 'workspace_root':f'/synthetic/project-{(i//4)%20:02d}',
            'project_branch':'main', 'path':f'/synthetic/session-{i}.jsonl',
            'environment_id':'native' if (i//4)%2 else 'mounted', 'environment':'Native' if (i//4)%2 else 'Mounted',
            'start_at':timestamp(i), 'end_at':timestamp(i+10), 'updated_at':timestamp(i+10),
            'total_token_usage':u(i%311+1), 'last_token_usage':u(1),
            'estimated_cost_usd': None if i%13==0 else round(i*.00123,6), 'price_model_known':i%13!=0,
            'cached_input_percent':60, 'turn_count':i%30+1, 'completed_turn_count':i%30+1}

def config():
    schedules=lambda rows: [[d.utc_iso(t) if t else None,p] for t,p in rows]
    return {'schedules':schedules(d.MODEL_PRICE_SCHEDULES), 'fast':d.FAST_MODE_COST_MULTIPLIERS_BY_MODEL,
            'ultrafast':d.ULTRAFAST_MODE_COST_MULTIPLIERS_BY_MODEL,
            'families':[{'short':d.GPT_5_6_MODEL_PRICES_USD_PER_M_TOKENS,'threshold':d.GPT_5_6_LONG_CONTEXT_INPUT_THRESHOLD,
                         'schedules':schedules(d.GPT_5_6_LONG_CONTEXT_PRICE_SCHEDULES)},
                        {'short':d.GPT_6_MODEL_PRICES_USD_PER_M_TOKENS,'threshold':d.GPT_6_LONG_CONTEXT_INPUT_THRESHOLD,
                         'long':d.GPT_6_LONG_CONTEXT_MODEL_PRICES_USD_PER_M_TOKENS},
                        {'short':d.GROK_4_5_MODEL_PRICES_USD_PER_M_TOKENS,'threshold':d.GROK_4_5_LONG_CONTEXT_INPUT_THRESHOLD,
                         'long':d.GROK_4_5_LONG_CONTEXT_MODEL_PRICES_USD_PER_M_TOKENS}]}

def generate():
    WORK.mkdir(exist_ok=True)
    cases=[]
    def case(name, op, **kw):
        path=WORK/'cases'/f'{name}.json'; dump(path,dict(op=op,**kw));cases.append({'name':name,'op':op,'path':str(path)})
    for size in [1000,10000]:
        rows=[session(i) for i in range(size)]
        case(f'summary_{size}','summary',sessions=rows)
        case(f'summary_typed_{size}','summary_typed',sessions=rows)
        payload={'sessions':rows,'summary':d.CodexUsageAnalyzer.build_summary_static(rows)}
        case(f'serialize_{size}','serialize',payload=payload,output_json=str(WORK/f'rust-{size}.json'))
        case(f'cache_encode_{size}','encode',payload=payload,output_blob=str(WORK/f'rust-{size}.zlib'))
        blob=WORK/f'python-{size}.zlib';blob.write_bytes(d.PersistentParseCache.encode_payload(payload))
        case(f'cache_decode_{size}','decode',blob=str(blob))
        dump(WORK/f'frontend-{size}.json',payload)
    dates=[None,'invalid','2026-01-01T00:00:00Z']
    for t,_ in d.MODEL_PRICE_SCHEDULES:
        if t: dates += [d.utc_iso(t-dt.timedelta(seconds=1)),d.utc_iso(t),d.utc_iso(t+dt.timedelta(seconds=1))]
    models=list(d.MODEL_PRICES_USD_PER_M_TOKENS)+['unknown','router/gpt-5.6-sol','gpt-daybreak-blue-latest','gpt-daybreak-red-latest','gpt-6.1-sol-2026-09-29']
    rng=random.Random(20261021)
    combos=list(itertools.product(models,dates,[0,272000,272001,None,300000],['','priority','ultrafast','fast','standard']))
    rng.shuffle(combos)
    prices=[dict(zip(['model','timestamp','input_tokens','service_tier'],c)) for c in combos[:10000]]
    case('price_lookup_10000','pricing',cases=prices,config=config())
    costs=[{'usage':u(i%331), 'prices':{'input':2.5,'cached_input':.25,'output':15.}} for i in range(20000)]
    costs += [{'usage':dict(input_tokens=-1,cached_input_tokens=100,output_tokens=3,reasoning_output_tokens=99),'prices':{'input':2.5,'cached_input':.25,'output':15.}}]
    case('cost_arithmetic_20001','cost',cases=costs)
    case('cost_typed_20001','cost_typed',cases=costs)
    for count in [1000,10000]:
        tl=timeline(count,reset=True);tl[20],tl[21]=tl[21],tl[20]
        case(f'timeline_delta_{count}','timeline',timeline=tl)
        case(f'timeline_typed_{count}','timeline_typed',timeline=tl)
        case(f'daily_aggregation_{count}','daily',details=[{'timeline':tl}])
    parent=timeline(2000)
    forks=[{'parent':parent,'child':parent[500:1500]+timeline(120,reset=True)},
           {'parent':[],'child':timeline(5)}, {'parent':parent,'child':[]},
           {'parent':timeline(200,reset=True),'child':timeline(100,reset=True)}]
    case('fork_rebase','fork',cases=forks)
    rows=[session(i) for i in range(1000)]
    snapshot=lambda ss:{'generated_at':timestamp(100),'codex_home':'/synthetic','codex_sources':[],
                        'sessions':ss,'details_by_uid':{r['uid']:dict(r,timeline=timeline(10)) for r in ss}}
    incoming=[dict(session(i),uid=f'replaced-{i}') for i in range(500,1250)]
    case('remote_merge_1000_750','remote',existing=snapshot(rows),incoming=snapshot(incoming))
    home=WORK/'home'; (home/'sessions').mkdir(parents=True,exist_ok=True);(home/'archived_sessions').mkdir(exist_ok=True)
    for i in range(160):
        path=home/('archived_sessions' if i%5==0 else 'sessions')/f'rollout-{i:05d}.jsonl'
        with path.open('wb') as f:
            def line(v):f.write((json.dumps(v,ensure_ascii=False,separators=(',',':'))+'\n').encode())
            line({'timestamp':timestamp(0),'type':'session_meta','payload':{'id':f'synthetic-{i}','cwd':'/synthetic/project','model':'gpt-5'}})
            for j in range(80):
                line({'timestamp':timestamp(j),'type':'event_msg','payload':{'type':'token_count','info':{'total_token_usage':u(j+1),'last_token_usage':u(1)}}})
                if j%8==0:line({'timestamp':timestamp(j),'type':'response_item','payload':{'type':'function_call_output','output':'x'*8192}})
    large=WORK/'large.jsonl'
    with large.open('wb') as out:
        for p in sorted(home.rglob('*.jsonl'))[:80]:out.write(p.read_bytes())
    case('jsonl_projected_9mb','reader',path=str(large))
    tail=large.stat().st_size
    with large.open('ab') as f:
        for j in range(32):f.write((json.dumps({'timestamp':timestamp(j),'type':'event_msg','payload':{'type':'token_count','info':{'total_token_usage':u(j+1)}}},separators=(',',':'))+'\n').encode())
    case('jsonl_append_32','reader',path=str(large),start=tail)
    edge=WORK/'edge.jsonl';edge.write_bytes(b'\xef\xbb\xbf'+b'{"type":"session_meta","payload":{"id":"edge"}}\n\n[1]\n{broken}\n'+
        '{"timestamp":"2026-10-01T16:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1.5,"cache_write_input_tokens":2,"total_tokens":3}}}}\n'.encode()+b'{"text":"\xff"}\n{"incomplete":')
    case('jsonl_edge_cases','reader',path=str(edge),return_rows=True)
    case('inventory_native_160','inventory',root=str(home))
    # This index is a disposable proposed schema, not a migration of user data.
    db=WORK/'query.sqlite';db.unlink(missing_ok=True)
    conn=sqlite3.connect(db);conn.execute('CREATE TABLE sessions(uid TEXT PRIMARY KEY, project TEXT, total INTEGER, timestamp TEXT)')
    conn.executemany('INSERT INTO sessions VALUES(?,?,?,?)',[(r['uid'],r['project'],r['total_token_usage']['total_tokens'],r['end_at']) for r in [session(i) for i in range(10000)]])
    conn.execute('CREATE INDEX project_total ON sessions(project,total DESC,uid)');conn.commit();conn.close()
    case('sqlite_indexed_top50','query',db=str(db),sql="SELECT uid,total FROM sessions WHERE project='project-01' ORDER BY total DESC,uid LIMIT 50")
    dump(WORK/'manifest.json',{'seed':20261021,'cases':cases,'fixture_bytes':sum(p.stat().st_size for p in home.rglob('*.jsonl')),'timezone':'Asia/Shanghai','home':str(home)})
    print(f'Generated {len(cases)} module cases; all data synthetic.')

def reader(c):
    stats=d.RolloutReadStats();counts={};total=d.zero_usage();errors=0;rows=[]
    for row in d.read_rollout_jsonl(Path(c['path']),c.get('start',0),stats,c.get('end')):
        p=row.get('payload') or {};key=f"{row.get('type','')}/{p.get('type','')}";counts[key]=counts.get(key,0)+1
        total=d.add_usage(total,d.normalize_usage((p.get('info') or {}).get('total_token_usage')))
        errors+=bool(row.get('__parse_error__'))
        if c.get('return_rows'):rows.append(row)
    return dict(bytes_read=stats.bytes_read,fast_skipped_line_count=stats.fast_skipped_line_count,fast_skipped_bytes=stats.fast_skipped_bytes,errors=errors,counts=counts,usage_checksum=total,rows=rows)

def operation(c):
    op=c['op'].removesuffix('_typed')
    if op=='reader':return lambda:reader(c)
    if op=='timeline':return lambda:[[r,u] for r,t,cu,u in d.timeline_rows_with_deltas(c['timeline'])]
    if op=='fork':
        def fork():
            results=[]
            for q in c['cases']:
                prefix=d.inherited_timeline_prefix_length(q['child'],q['parent'])
                rows,total,last,inherited=d.rebase_timeline_after_prefix(q['child'],prefix)
                results.append(dict(prefix=prefix,rebased=rows,total=total,last=last,inherited=inherited))
            return results
        return fork
    if op=='pricing':return lambda:[d.price_entry_for_model(q['model'],q['timestamp'],q['input_tokens'],q['service_tier']) for q in c['cases']]
    if op=='cost':return lambda:[d.usage_cost_parts(q['usage'],q['prices']) for q in c['cases']]
    if op=='summary':return lambda:d.CodexUsageAnalyzer.build_summary_static(c['sessions'])
    if op=='daily':return lambda:d.CodexUsageAnalyzer.build_daily_usage_static(c['details'])
    if op=='remote':
        store=d.RemoteSnapshotStore(current_device_code='synthetic',root=WORK/'remote')
        return lambda:store.merge_snapshots(c['existing'],c['incoming'])
    if op=='serialize':return lambda:{'bytes':len(json.dumps(c['payload'],ensure_ascii=False,separators=(',',':')).encode())}
    if op=='encode':return lambda:{'bytes':len(d.PersistentParseCache.encode_payload(c['payload']))}
    if op=='decode':
        blob=Path(c['blob']).read_bytes();return lambda:d.PersistentParseCache.decode_payload(blob)
    if op=='inventory':
        def inventory():
            rows=[]
            def walk(path):
                with os.scandir(path) as entries:
                    for e in entries:
                        if e.is_dir():walk(e.path)
                        elif e.name.endswith('.jsonl'):rows.append([e.path,e.stat().st_size])
            walk(c['root']);return sorted(rows)
        return inventory
    if op=='query':
        conn=sqlite3.connect(c['db']);return lambda:[list(r) for r in conn.execute(c['sql'])]
    raise ValueError(op)

def worker(path,samples):
    c=json.loads(Path(path).read_text());fn=operation(c);times=[]
    for i in range(samples+2):
        start=time.perf_counter();out=fn();elapsed=(time.perf_counter()-start)*1000
        result=out
        if i>=2:times.append(elapsed)
    # /proc matches Rust and resets at exec; ru_maxrss may include the launching
    # Python parent's fork-time high-water mark on this host.
    peak=int(next(line.split()[1] for line in Path('/proc/self/status').read_text().splitlines() if line.startswith('VmHWM:')))
    return dict(samples_ms=times,result=result,peak_rss_kib=peak)

def equal(a,b,path='root'):
    if isinstance(a,int) and isinstance(b,int):
        assert a==b,(path,a,b)
    elif isinstance(a,(float,int)) and isinstance(b,(float,int)):
        assert math.isclose(a,b,rel_tol=1e-12,abs_tol=1e-8),(path,a,b)
    elif isinstance(a,dict):
        assert isinstance(b,dict) and a.keys()==b.keys(),(path,'keys',a.keys(),b.keys() if isinstance(b,dict) else type(b))
        for k in a:equal(a[k],b[k],f'{path}.{k}')
    elif isinstance(a,list):
        assert isinstance(b,list) and len(a)==len(b),(path,'length',len(a),len(b))
        for i,(x,y) in enumerate(zip(a,b)):equal(x,y,f'{path}[{i}]')
    else:assert a==b,(path,a,b)

def stats(times):
    return dict(median_ms=statistics.median(times),p95_ms=sorted(times)[math.ceil(len(times)*.95)-1],min_ms=min(times),max_ms=max(times),samples_ms=times)

def baseline(samples=5):
    home=WORK/'home';measurements={}
    def measure(fn,count=samples):
        ts=[]
        for _ in range(count):
            start=time.perf_counter();result=fn();ts.append((time.perf_counter()-start)*1000)
        return stats(ts),result
    def cold():
        a=d.CodexUsageAnalyzer(home,resolve_project_info=False);snapshot=a.scan('all');a.close();return snapshot
    measurements['full_scan_no_parse_cache'],snapshot=measure(cold)
    cache=WORK/'production-cache.sqlite';cache.unlink(missing_ok=True)
    a=d.CodexUsageAnalyzer(home,persistent_cache=d.PersistentParseCache(cache),resolve_project_info=False)
    measurements['first_scan_with_cache_write'],_=measure(lambda:a.scan('all'),1)
    measurements['unchanged_scan'],_=measure(lambda:a.scan('all'),15)
    uid=snapshot['sessions'][0]['uid'];token=a.scan('all')['snapshot_token']
    measurements['detail_snapshot_lookup'],_=measure(lambda:a.get_detail(uid,snapshot_token=token),100)
    target=next((home/'sessions').glob('*.jsonl'));original=target.read_bytes();append_times=[]
    for i in range(samples):
        row={'timestamp':timestamp(100+i),'type':'event_msg','payload':{'type':'token_count','info':{'total_token_usage':u(81+i),'last_token_usage':u(1)}}}
        with target.open('ab') as f:f.write((json.dumps(row,separators=(',',':'))+'\n').encode())
        start=time.perf_counter();updated=a.scan('all');append_times.append((time.perf_counter()-start)*1000)
        assert updated['summary']['usage']['total_tokens']==160*80*1120+(i+1)*1120
    target.write_bytes(original);measurements['append_one_event_scan']=stats(append_times);a.close()
    def persistent():
        a=d.CodexUsageAnalyzer(home,persistent_cache=d.PersistentParseCache(cache),resolve_project_info=False);snap=a.scan('all');a.close();return snap
    measurements['restart_with_persistent_cache'],_=measure(persistent)
    measurements['full_scan_no_parse_cache']['session_count']=len(snapshot['sessions'])
    return measurements

def run(samples):
    manifest=json.loads((WORK/'manifest.json').read_text());results=[]
    exe=HERE/'rust/target/release/dashboard-refactor-probe'
    report={'environment':{'platform':platform.platform(),'python':sys.version,'rust':subprocess.check_output(['rustc','--version'],text=True).strip(),
                           'commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'source_sha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
                           'timezone':os.environ['TZ'],'sqlite_python':sqlite3.sqlite_version},'method':{'samples':samples,'warmups':2,'fixture_seed':20261021,
                            'os_cache':'warm; no privileged cache drop','timing':'operation only, input JSON load and process startup excluded; output serialization excluded',
                            'rss':'whole worker peak including fixture load and retained results; not incremental module allocation'},'results':results}
    for index,entry in enumerate(manifest['cases']):
        c=json.loads(Path(entry['path']).read_text());data={}
        for lang in (['python','rust'] if index%2==0 else ['rust','python']):
            cmd=[sys.executable,str(__file__),'worker',entry['path'],'--samples',str(samples)] if lang=='python' else [str(exe),entry['path'],str(samples)]
            data[lang]=json.loads(subprocess.check_output(cmd,text=True,cwd=ROOT))
        if c['op']=='encode':
            equal(c['payload'],json.loads(zlib.decompress(Path(c['output_blob']).read_bytes())))
            parity='both codecs roundtrip; compressed sizes may differ'
        elif c['op']=='serialize':
            equal(c['payload'],json.loads(Path(c['output_json']).read_text()))
            parity='JSON roundtrip passed; encoding byte counts may differ'
        else:
            equal(data['python']['result'],data['rust']['result']);parity='passed'
        result={'name':entry['name'],'operation':c['op'],'parity':parity}
        for lang in data:result[lang]=dict(stats(data[lang]['samples_ms']),peak_rss_kib=data[lang]['peak_rss_kib'])
        if c['op'] in ('encode','serialize'):
            for lang in data:result[lang]['output_bytes']=data[lang]['result']['bytes']
        result['speedup_python_over_rust']=result['python']['median_ms']/result['rust']['median_ms']
        results.append(result);dump(HERE/'results/backend.json',report)
        print(f"{entry['name']}: Python {result['python']['median_ms']:.3f}ms / Rust {result['rust']['median_ms']:.3f}ms = {result['speedup_python_over_rust']:.2f}x; {parity}",flush=True)
    report['production_baseline']=baseline();dump(HERE/'results/backend.json',report)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('command',choices=['generate','run','worker']);p.add_argument('case',nargs='?');p.add_argument('--samples',type=int,default=9);args=p.parse_args()
    if args.command=='generate':generate()
    elif args.command=='worker':print(json.dumps(worker(args.case,args.samples),ensure_ascii=False,separators=(',',':')))
    else:run(args.samples)
