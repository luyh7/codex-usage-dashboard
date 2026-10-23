"""Run legacy behavioral tests with only parse_file replaced by the Rust core.

This is a test adapter, not a runtime fallback. Python still performs indexing,
parent/continuation normalization and runtime-tier merging in these tests.
"""
import json
from pathlib import Path
import subprocess
import sys
import threading
import unittest
from unittest.mock import patch
from compatibility import ROOT

sys.path.insert(0,str(ROOT/'tests'))
import test_codex_usage_dashboard as legacy


def main():
    process=subprocess.Popen([str(ROOT/'target/release/examples/contract')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,bufsize=1)
    lock=threading.Lock();calls=0
    def parse(analyzer,path,source,log_source=None,base_detail=None,start_offset=0,end_offset=None):
        nonlocal calls
        log_source=log_source or analyzer.codex_sources[0]
        payload={'op':'parse_file','path':str(path),'home':str(log_source.codex_home),'source_id':log_source.id,'source_label':log_source.label,'kind':source,'end':end_offset,'return_state':True}
        fields={'session_meta_seen':'_session_meta_seen','raw_created_at':'_raw_created_at','raw_start_at':'_raw_start_at','raw_end_at':'_raw_end_at','turn_ids':'_turn_ids','service_tier_events':'_service_tier_events','parsed_end_offset':'_parsed_end_offset'}
        if base_detail:
            state={key:base_detail[private] for key,private in fields.items()}
            state['parsed_end_offset']=start_offset
            payload['base']={'detail':{k:v for k,v in base_detail.items() if not k.startswith('_')},'state':state}
        with lock:
            calls+=1;process.stdin.write(json.dumps(payload)+'\n');process.stdin.flush()
            response=json.loads(process.stdout.readline())
        if 'error' in response:raise AssertionError(response['error'])
        detail=response['detail']
        detail.update({private:response['state'][key] for key,private in fields.items()})
        return {key:detail[key] for key in legacy.dashboard.SUMMARY_KEYS},detail
    names=[name for name in dir(legacy.CodexUsageDashboardTests) if name.startswith('test_') and any(term in name for term in ['parse_file','subagent','parent','continued','large_payloads','append_only','append_resume','incomplete_appended','session_models','thread_settings_applied','cross_launch','runtime_log_tier_reprices','gpt_6_to_6_1','scan_combines','windows_cwd','replaced_file_identity','truncated_file'])]
    try:
        with patch.object(legacy.dashboard.CodexUsageAnalyzer,'parse_file',parse):
            result=unittest.TextTestRunner(verbosity=2).run(unittest.TestSuite(legacy.CodexUsageDashboardTests(name) for name in names))
        report={'status':'passed' if result.wasSuccessful() else 'failed','tests':names,'rust_parse_calls':calls,'scope':__doc__.strip()}
        (ROOT/'benchmarks/implementation/results/parser-bridge.json').write_text(json.dumps(report,indent=2)+'\n')
        if not result.wasSuccessful():raise SystemExit(1)
    finally:
        process.stdin.close();process.wait(timeout=5)


if __name__=='__main__':main()
