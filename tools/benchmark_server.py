"""Linux/WSL end-to-end startup, metadata and five-minute idle evidence.
Synthetic-only, fixed port 8765; runs workers sequentially and restores worktree.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import tempfile
import time
from benchmark_index import generate,run,stats
from verify_server import ROOT,SCRIPT,stop_owned,request
sys.path.insert(0,str(ROOT/'benchmarks/refactor'))
from verify_dashboard import verify
BINARY=ROOT/'target/release/codex-usage-dashboard'

def sample(pid):
    status=Path(f'/proc/{pid}/status').read_text();memory={}
    for line in status.splitlines():
        if line.startswith(('VmRSS:','VmHWM:')):memory[line.split(':')[0]+'_kib']=int(line.split()[1])
    stat=Path(f'/proc/{pid}/stat').read_text().split(') ',1)[1].split();memory['cpu_seconds']=(int(stat[11])+int(stat[12]))/os.sysconf('SC_CLK_TCK');memory['fds']=len(list(Path(f'/proc/{pid}/fd').iterdir()));return memory

def start(variant,home,config):
    command=[sys.executable,str(SCRIPT)] if variant=='python' else [str(BINARY),'--serve']
    command+=['--codex-home',str(home),'--no-auto-windows'];env={**os.environ,'COUSASH_CONFIG_DIR':str(config),'TZ':'UTC'}
    before=time.perf_counter();proc=subprocess.Popen(command,cwd=ROOT,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    deadline=time.monotonic()+30
    try:
        while True:
            if proc.poll() is not None:raise RuntimeError(f'{variant} exited {proc.returncode}')
            try:
                status,value=request('/api/sessions?period=all')
                if status==200:break
            except OSError:pass
            if time.monotonic()>deadline:raise RuntimeError('startup timeout')
            time.sleep(.005)
        return proc,{'samples_ms':[(time.perf_counter()-before)*1000],'sessions':len(value['sessions']),'tokens':value['summary']['usage']['total_tokens'],**sample(proc.pid)}
    except BaseException:
        proc.send_signal(signal.SIGINT);proc.wait(timeout=15);raise

def stop(proc):
    if proc and proc.poll() is None:proc.send_signal(signal.SIGINT);proc.wait(timeout=15)

def main():
    subprocess.run(['cargo','build','--release','--bin','codex-usage-dashboard','--example','index_benchmark'],cwd=ROOT,check=True)
    report={'platform':platform.platform(),'python':platform.python_version(),'rust':subprocess.check_output(['rustc','--version'],text=True).strip(),'build':json.loads(subprocess.check_output([str(BINARY),'--build-info'],text=True)),'binary_sha256':hashlib.sha256(BINARY.read_bytes()).hexdigest(),'python_sha256':hashlib.sha256(SCRIPT.read_bytes()).hexdigest(),'samples':15,'warmups':2,'method':'Sequential processes on fixed 8765; warm OS cache, synthetic 160 files x 80 events. Startup includes process launch, index restore or full scan, first legacy list HTTP response and JSON decode. No browser in startup measurement. Idle has no UI clients; 10-second health-only sampling. Mounted metadata uses 2000 historical files and cached inventories.','startup':{},'inventory':{}}
    proc=None
    try:
        stop_owned()
        with tempfile.TemporaryDirectory(prefix='server-benchmark-') as temp:
            root=Path(temp);home=root/'home';generate(home)
            for operation in ['cold','restore']:
                variants={}
                for variant in ['python','rust']:
                    workers=[];config=root/(variant+'-'+operation)
                    if operation=='restore':proc,_=start(variant,home,config);stop(proc);proc=None
                    for i in range(17):
                        location=config if operation=='restore' else root/f'{variant}-cold-{i}'
                        proc,result=start(variant,home,location);stop(proc);proc=None
                        assert result['sessions']==160 and result['tokens']==160*80*1120
                        if i>=2:workers.append(result)
                    variants[variant]={**stats([w['samples_ms'][0] for w in workers]),'peak_rss_kib':max(w['VmHWM_kib'] for w in workers),'workers':workers}
                report['startup'][operation]=variants;print(operation,{k:round(v['p50_ms'],2) for k,v in variants.items()},flush=True)
            for label,parent in [('native',None),('wsl-mounted',Path(os.environ.get('COUSASH_WINDOWS_TEMP','/nonexistent-windows-temp')))]:
                if parent and not parent.is_dir():report['inventory'][label]={'unrun':'mount unavailable'};continue
                with tempfile.TemporaryDirectory(prefix='cousash-metadata-',dir=parent) as tmp:
                    path=Path(tmp)/'home';folder=path/'sessions';folder.mkdir(parents=True)
                    for i in range(2000):
                        file=folder/f'rollout-{i:05}.jsonl';file.write_text('{}\n');os.utime(file,(1700000000,1700000000))
                    report['inventory'][label]={}
                    for variant in ['python','rust']:
                        result=run(variant,path,root/(variant+'-'+label+'.sqlite3'),'inventory');assert result['files']==2000;report['inventory'][label][variant]={**stats(result['samples_ms']),**result}
            proc,_=start('rust',home,root/'rust-idle');time.sleep(3);before=request('/api/health')[1];points=[];start_time=time.monotonic()
            duration=int(os.environ.get('COUSASH_IDLE_SECONDS','300'))
            for i in range(duration//10+1):
                if i:time.sleep(10)
                points.append({'elapsed_seconds':time.monotonic()-start_time,**sample(proc.pid)})
                if i%3==0:print('idle',round(points[-1]['elapsed_seconds']),points[-1]['VmRSS_kib'],points[-1]['fds'],flush=True)
            after=request('/api/health')[1];assert before['index']['revision']==after['index']['revision'];assert before['index']['metrics']['full_parses']==after['index']['metrics']['full_parses']
            report['idle']={'duration_seconds':duration,'samples':points,'before':before['index'],'after':after['index'],'cpu_seconds':points[-1]['cpu_seconds']-points[0]['cpu_seconds'],'rss_growth_kib':points[-1]['VmRSS_kib']-points[0]['VmRSS_kib'],'fd_growth':points[-1]['fds']-points[0]['fds'],'no_reparse':True};stop(proc);proc=None
    finally:
        stop(proc);verify()
    output=ROOT/'benchmarks/implementation/results/server-performance.json';output.write_text(json.dumps(report,indent=2)+'\n');print(output)
if __name__=='__main__':main()
