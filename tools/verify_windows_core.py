"""Run GNU-target Rust unit executables as native Windows processes from WSL.
Only temporary synthetic data is used. This does not cover the native browser,
installer, MSVC runtime, or constitute complete Windows release acceptance.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
ROOT=Path(__file__).resolve().parents[1]
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--temp-root',type=Path,required=True);args=parser.parse_args()
    build=subprocess.run(['cargo','test','--workspace','--target','x86_64-pc-windows-gnu','--no-run','--message-format=json'],cwd=ROOT,capture_output=True,text=True,check=True)
    artifacts=[v for line in build.stdout.splitlines() if (v:=json.loads(line)).get('reason')=='compiler-artifact' and v.get('executable') and v.get('profile',{}).get('test')]
    report={'scope':__doc__,'target':'x86_64-pc-windows-gnu','runs':[]}
    with tempfile.TemporaryDirectory(prefix='cousash-native-core-',dir=args.temp_root) as temp:
        for artifact in artifacts:
            source=Path(artifact['executable']);target=Path(temp)/source.name;shutil.copy2(source,target)
            result=subprocess.run([str(target),'--test-threads=1'],cwd=temp,capture_output=True,timeout=90)
            report['runs'].append({'name':artifact['target']['name'],'sha256':hashlib.sha256(target.read_bytes()).hexdigest(),'exit_code':result.returncode,'stdout':result.stdout.decode('utf-8','replace'),'stderr':result.stderr.decode('utf-8','replace')})
    report['passed']=bool(artifacts) and all(r['exit_code']==0 for r in report['runs']);output=ROOT/'benchmarks/implementation/results/windows-core.json';output.write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n');print(json.dumps(report,ensure_ascii=False))
    if not report['passed']:raise SystemExit(1)
if __name__=='__main__':main()
