"""Measure native/mounted I/O and query architecture using disposable data."""
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import time
from backend import d, WORK, HERE, dump, equal, operation, stats, session

def measure(fn,n=15):
    ts=[]
    for i in range(n+2):
        start=time.perf_counter();result=fn();elapsed=(time.perf_counter()-start)*1000
        if i>=2:ts.append(elapsed)
    return stats(ts),result

def main():
    report={'method':'warm OS caches; disposable synthetic files; no native Windows/macOS execution','io':[],'query':{}}
    parent=Path('/mnt/c/Users/luyh7/AppData/Local/Temp')
    for label,base in [('native',WORK),('wsl_windows_mount',parent)]:
        if not base.is_dir():report['io'].append({'location':label,'unavailable':True});continue
        with tempfile.TemporaryDirectory(prefix='dashboard-refactor-probe-',dir=base) as folder:
            home=Path(folder);(home/'sessions').mkdir();(home/'archived_sessions').mkdir()
            for i in range(2000):
                path=home/'sessions'/f'rollout-{i}.jsonl';path.write_text('{}\n')
            c={'op':'inventory','root':folder};case=WORK/'io-case.json';dump(case,c)
            py,_=measure(operation(c));rs=json.loads(subprocess.check_output([str(HERE/'rust/target/release/dashboard-refactor-probe'),str(case),'15'],text=True))
            equal(operation(c)(),rs['result'])
            a=d.CodexUsageAnalyzer(home,resolve_project_info=False)
            current_active,_=measure(a.iter_session_files)
            for p in home.rglob('*.jsonl'):os.utime(p,(1_700_000_000,1_700_000_000))
            a=d.CodexUsageAnalyzer(home,resolve_project_info=False)
            current_history,_=measure(a.iter_session_files)
            large=home/'read.jsonl';shutil.copyfile(WORK/'large.jsonl',large)
            rc={'op':'reader','path':str(large)};dump(case,rc)
            read_py,_=measure(operation(rc));read_rs=json.loads(subprocess.check_output([str(HERE/'rust/target/release/dashboard-refactor-probe'),str(case),'15'],text=True))
            equal(operation(rc)(),read_rs['result'])
            rc['buffer_bytes']=131072;dump(case,rc)
            buffered=json.loads(subprocess.check_output([str(HERE/'rust/target/release/dashboard-refactor-probe'),str(case),'15'],text=True))
            equal(operation(rc)(),buffered['result'])
            report['io'].append({'location':label,'file_count':2000,'python_scandir':py,'rust_read_dir':stats(rs['samples_ms']),
                'current_inventory_active':current_active,'current_inventory_historical':current_history,
                'log_bytes':large.stat().st_size,'python_reader':read_py,'rust_reader':stats(read_rs['samples_ms']),
                'rust_reader_128k_buffer':stats(buffered['samples_ms'])})
    rows=[session(i) for i in range(10000)]
    def linear():return [[r['uid'],r['total_token_usage']['total_tokens']] for r in sorted((r for r in rows if r['project']=='project-01'),key=lambda r:(-r['total_token_usage']['total_tokens'],r['uid']))[:50]]
    report['query']['python_in_memory_top50'],expected=measure(linear)
    sql="SELECT uid,total FROM sessions WHERE project='project-01' ORDER BY total DESC,uid LIMIT 50"
    conn=sqlite3.connect(':memory:')
    start=time.perf_counter();conn.execute('CREATE TABLE sessions(uid TEXT PRIMARY KEY,project TEXT,total INTEGER)')
    conn.executemany('INSERT INTO sessions VALUES(?,?,?)',[(r['uid'],r['project'],r['total_token_usage']['total_tokens']) for r in rows]);conn.execute('CREATE INDEX ptotal ON sessions(project,total DESC,uid)');conn.commit()
    report['query']['index_build_ms']=(time.perf_counter()-start)*1000
    report['query']['python_indexed_top50'],actual=measure(lambda:[list(r) for r in conn.execute(sql)])
    equal(expected,actual)
    report['query']['full_sessions_json_bytes']=len(json.dumps(rows,ensure_ascii=False,separators=(',',':')).encode())
    selected={r[0] for r in actual};page=[r for r in rows if r['uid'] in selected]
    report['query']['page50_sessions_json_bytes']=len(json.dumps(page,ensure_ascii=False,separators=(',',':')).encode())
    report['query']['parity']='exact ordered top-50 IDs and total tokens; excludes full production filtering/project tree semantics'
    dump(HERE/'results/io-query.json',report)
    print(json.dumps(report,ensure_ascii=False,indent=2))

if __name__=='__main__':main()
