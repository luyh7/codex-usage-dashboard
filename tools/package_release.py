"""Build a self-contained native artifact with embedded Svelte assets and hashes.
Run natively on each supported platform; cross-build success is not acceptance.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'skills/codex-usage-dashboard/scripts'))
import dashboard_runtime

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,default=ROOT/'dist/release');parser.add_argument('--skip-build',action='store_true');args=parser.parse_args()
    if not args.skip_build:
        subprocess.run(['npm.cmd' if os.name=='nt' else 'npm','ci'],cwd=ROOT/'web',check=True)
        subprocess.run(['npm.cmd' if os.name=='nt' else 'npm','run','build'],cwd=ROOT/'web',check=True)
        subprocess.run(['cargo','build','--release','--locked','--bin','codex-usage-dashboard'],cwd=ROOT,env={**os.environ,'COUSASH_REQUIRE_WEB':'1'},check=True)
    binary=ROOT/'target/release'/('codex-usage-dashboard.exe' if os.name=='nt' else 'codex-usage-dashboard')
    info=json.loads(subprocess.check_output([str(binary),'--build-info'],text=True));key=dashboard_runtime.platform_key()
    name=f'codex-usage-dashboard-{info["version"]}-{key}-{info["build_id"][:12]}'+('.exe' if os.name=='nt' else '')
    args.output.mkdir(parents=True,exist_ok=True);target=args.output/name;shutil.copy2(binary,target)
    entry={'file':name,'sha256':hashlib.sha256(target.read_bytes()).hexdigest(),'build_id':info['build_id']}
    manifest={'version':info['version'],'protocol_version':1,'platforms':{key:entry}}
    (args.output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
    (args.output/'SHA256SUMS').write_text(f'{entry["sha256"]}  {name}\n',encoding='utf-8')
    print(json.dumps(manifest))
if __name__=='__main__':main()
