"""Compare v1 import/merge and sparse old archives against existing behavior."""
import copy
import datetime as dt
import hashlib
import json
import os
import time
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
from compatibility import ROOT,compare
sys.path.insert(0,str(ROOT/'tests'))
import test_codex_usage_dashboard as legacy

def volatile(value):
    if isinstance(value,list):return [volatile(v) for v in value]
    if isinstance(value,tuple):return [volatile(v) for v in value]
    if isinstance(value,dict):
        result={k:volatile(v) for k,v in value.items()}
        for key in ['imported_at','remote_imported_at']:
            if result.get(key):assert legacy.dashboard.parse_timestamp(result[key]);result[key]='<IMPORT_TIME>'
        return result
    return value

def main():
    os.environ["TZ"]="UTC";time.tzset()
    process=subprocess.Popen([str(ROOT/'target/release/examples/contract')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,bufsize=1)
    store=legacy.dashboard.RemoteSnapshotStore;original_import=store.import_snapshot;original_transform=store.transformed_sessions
    checks=[]
    with tempfile.TemporaryDirectory(prefix='remote-bridge-') as tmp:
        def root(instance):return str(Path(tmp)/hashlib.sha256(str(instance.root).encode()).hexdigest())
        def call(instance,**args):
            process.stdin.write(json.dumps({'op':'remote_store','root':root(instance),'device':instance.current_device_code,**args})+'\n');process.stdin.flush();return json.loads(process.stdout.readline())
        def import_snapshot(instance,incoming,label=None,allow_current_device=False):
            expected=original_import(instance,incoming,label=label,allow_current_device=allow_current_device)
            actual=call(instance,action='import',snapshot=incoming,label=label,allow_current_device=allow_current_device)
            compare(volatile(expected),volatile(actual));checks.append('import/confirmation/merge')
            if expected.get('ok'):
                code=expected['remote']['device_short_code'];stored=json.loads((Path(root(instance))/(code+'.json')).read_text());compare(volatile(instance.read_remote(code)),volatile(stored));checks.append('stored-v1-archive')
            return expected
        def transformed(instance):
            expected=original_transform(instance);actual=call(instance,action='transformed');compare(volatile(expected),volatile(actual));checks.append('all-fields-transformed-sparse-v1')
            analyzer=legacy.dashboard.CodexUsageAnalyzer.__new__(legacy.dashboard.CodexUsageAnalyzer)
            for start,end in [('2020-01-01','2030-12-31'),('2026-10-01','2026-10-01')]:
                snapshot={'sessions':expected[0],'details_by_uid':expected[1]}
                ranged=analyzer.filter_snapshot_by_period(snapshot,'custom',start,end)
                actual=call(instance,action='ranged',start=start,end=end,now=ranged['period']['end_at'])
                compare(volatile({'sessions':ranged['sessions'],'details_by_uid':ranged['details_by_uid']}),volatile(actual));checks.append('sparse-v1-period')
            return expected
        names=['test_old_remote_snapshot_preserves_stored_cost_and_normalizes_cache_write_alias','test_remote_snapshot_reprices_fast_mode_from_timeline','test_remote_import_merges_by_device_short_code','test_remote_scan_marks_imported_sessions']
        def sparse_segments():
            instance=store('local-fixture',Path(tmp)/'old-segment-source')
            fixture=json.loads((ROOT/'tests/fixtures/remote-old-price-segments.json').read_text())
            instance.import_snapshot(fixture,label='Synthetic old segments')
            instance.transformed_sessions()
        try:
            with patch.object(store,'import_snapshot',import_snapshot),patch.object(store,'transformed_sessions',transformed):
                result=unittest.TextTestRunner(verbosity=2).run(unittest.TestSuite([*[legacy.CodexUsageDashboardTests(name) for name in names],unittest.FunctionTestCase(sparse_segments)]))
            report={'passed':result.wasSuccessful(),'tests':names+['sparse-old-price-segments'],'checks':checks,'normalizations':['only import time; validated ISO timestamp'],'scope':__doc__}
            (ROOT/'benchmarks/implementation/results/remote-bridge.json').write_text(json.dumps(report,indent=2)+'\n')
            if not result.wasSuccessful():raise SystemExit(1)
        finally:process.stdin.close();process.wait(timeout=5)
if __name__=='__main__':main()
