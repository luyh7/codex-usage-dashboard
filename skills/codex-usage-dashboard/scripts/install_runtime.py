#!/usr/bin/env python3
"""Explicit, checksum-verified native installation; launch never uses the network."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from urllib.error import URLError
from urllib.request import urlopen
import dashboard_runtime as runtime

def install(manifest_path, offline=False):
    manifest_path=Path(manifest_path).expanduser().resolve()
    data=json.loads(manifest_path.read_text(encoding='utf-8'))
    entry=data.get('platforms',{}).get(runtime.platform_key())
    if not entry:raise RuntimeError(f'No precompiled artifact for {runtime.platform_key()}; Python fallback remains available.')
    name=entry['file']
    if Path(name).name!=name:raise RuntimeError('Invalid artifact filename')
    root=runtime.runtime_root();root.mkdir(parents=True,exist_ok=True)
    local=manifest_path.parent/name
    with tempfile.TemporaryDirectory(prefix='.install-',dir=root) as temp:
        binary=Path(temp)/name
        if local.is_file():shutil.copyfile(local,binary)
        elif offline:raise RuntimeError(f'Offline: place {name} beside the manifest; current installation was preserved.')
        else:
            url=entry.get('url','')
            if not url.startswith('https://'):raise RuntimeError('Release manifest needs an HTTPS artifact URL or adjacent local artifact.')
            try:
                with urlopen(url,timeout=30) as source,binary.open('wb') as target:shutil.copyfileobj(source,target)
            except (OSError,URLError) as error:raise RuntimeError(f'Download failed; current installation was preserved. For offline installation place {name} beside the manifest. {error}') from error
        if hashlib.sha256(binary.read_bytes()).hexdigest()!=entry['sha256']:raise RuntimeError('Artifact checksum mismatch; current installation was preserved.')
        binary.chmod(0o755)
        info=json.loads(subprocess.check_output([str(binary),'--build-info'],text=True,timeout=10))
        if info.get('build_id')!=entry['build_id'] or info.get('version')!=data['version']:raise RuntimeError('Artifact build identity mismatch; current installation was preserved.')
        # Version/build-specific filenames keep the active and rollback binaries intact.
        target=root/name
        if target.exists() and hashlib.sha256(target.read_bytes()).hexdigest()!=entry['sha256']:raise RuntimeError('Refusing to overwrite a different artifact under the same name.')
        if not target.exists():os.replace(binary,target)
        manifest=root/'manifest.json'
        if manifest.exists():shutil.copyfile(manifest,root/'previous-manifest.json')
        staged=Path(temp)/'manifest.json';staged.write_text(json.dumps(data,indent=2)+'\n',encoding='utf-8');os.replace(staged,manifest)
    return target

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('manifest',nargs='?');parser.add_argument('--offline',action='store_true');parser.add_argument('--rollback',action='store_true');args=parser.parse_args()
    try:
        if args.rollback:
            root=runtime.runtime_root();previous=root/'previous-manifest.json'
            if not previous.is_file():raise RuntimeError('No previous native manifest. Use COUSASH_BACKEND=python for the bundled fallback.')
            data=json.loads(previous.read_text(encoding='utf-8'));install(previous,offline=True);print('Restored previous native manifest.')
        elif args.manifest:print(install(args.manifest,args.offline))
        else:parser.error('provide a local release manifest, or --rollback')
    except (OSError,ValueError,KeyError,RuntimeError,subprocess.SubprocessError) as error:print(str(error),file=sys.stderr);return 1
    return 0
if __name__=='__main__':raise SystemExit(main())
