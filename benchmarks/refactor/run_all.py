"""One-command, sequential reproduction of the pre-refactor evaluation."""
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from verify_dashboard import verify, ROOT

HERE=Path(__file__).resolve().parent
RESULTS=HERE/'results'

def main():
    RESULTS.mkdir(exist_ok=True)
    env=os.environ.copy();env['TZ']='Asia/Shanghai'
    stages=[]
    def run(name,command,cwd=ROOT):
        print(f'RUN {name}',flush=True)
        log=RESULTS/(name+'.log')
        with log.open('w') as stream:
            result=subprocess.run(command,cwd=cwd,env=env,stdout=stream,stderr=subprocess.STDOUT)
        stages.append({'stage':name,'exit_code':result.returncode,'command':command})
        if result.returncode:raise RuntimeError(f'{name} failed; see {log}')
    health=verify()
    run('production-tests',[sys.executable,'-m','unittest','discover','-s','tests','-v'])
    run('installer-check',['npm','run','validate'])
    run('rust-build',['cargo','build','--release','--locked','--manifest-path',str(HERE/'rust/Cargo.toml')])
    run('frontend-install',['npm','ci'],HERE/'frontend')
    run('frontend-build',['npm','run','build'],HERE/'frontend')
    run('fixtures',[sys.executable,str(HERE/'backend.py'),'generate'])
    run('backend-bench',[sys.executable,str(HERE/'backend.py'),'run','--samples','15'])
    run('differential-tests',[sys.executable,str(HERE/'correctness.py')])
    run('io-query-bench',[sys.executable,str(HERE/'io_query.py')])
    run('http-profile',[sys.executable,str(HERE/'http_profile.py')])
    run('frontend-bench',['npm','run','bench'],HERE/'frontend')
    health=verify(restart=False)
    tests=(RESULTS/'production-tests.log').read_text();count=int(re.search(r'Ran (\d+) tests',tests)[1])
    probe_sources=[p for p in HERE.rglob('*') if p.is_file() and p.suffix in ('.py','.rs','.mjs','.js','.svelte','.toml','.json','.lock') and not any(x in p.parts for x in ('.work','target','node_modules','results','dist'))]
    evidence={
        'status':'passed','completed_at':dt.datetime.now(dt.timezone(dt.timedelta(hours=8))).isoformat(),
        'stages':stages,'production_tests':count,'health':health,
        'cpu_model':next((l.split(':',1)[1].strip() for l in Path('/proc/cpuinfo').read_text().splitlines() if l.startswith('model name')),''),
        'cpu_count':os.cpu_count(),'rust_probe_sha256':hashlib.sha256((HERE/'rust/target/release/dashboard-refactor-probe').read_bytes()).hexdigest(),
        'probe_source_sha256':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in probe_sources},
        'result_sha256':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in RESULTS.glob('*.json') if p.name!='verification.json'},
    }
    (RESULTS/'verification.json').write_text(json.dumps(evidence,indent=2))
    run('report',[sys.executable,str(HERE/'report.py')])
    print('PASS: complete evidence in benchmarks/refactor/results; dashboard restored on 8765.',flush=True)

if __name__=='__main__':main()
