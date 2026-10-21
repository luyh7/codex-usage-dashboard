"""Differential gates against the frozen legacy domain semantics, synthetic only."""
from __future__ import annotations
import base64
import datetime as dt
import hashlib
import itertools
import json
import os
from pathlib import Path
import platform
import random
import sqlite3
import subprocess
import tempfile
import time

from compatibility import ROOT, load_dashboard, compare, canonical


def main():
    os.environ['TZ']='UTC'
    if hasattr(time,'tzset'): time.tzset()
    d=load_dashboard();rng=random.Random(64321)
    inputs=[];expected=[];names=[]
    def case(name,payload,result):
        names.append(name);inputs.append(payload);expected.append(result)
    dates=[None,'invalid','2026-01-01T00:00:00Z']
    for moment,_ in d.MODEL_PRICE_SCHEDULES:
        if moment:dates.extend(d.utc_iso(moment+dt.timedelta(seconds=x)) for x in [-1,0,1])
    models=list(d.MODEL_PRICES_USD_PER_M_TOKENS)+['unknown','router/gpt-5.6-sol','gpt-daybreak-blue-latest','gpt-daybreak-red-latest']
    thresholds=[d.GPT_5_6_LONG_CONTEXT_INPUT_THRESHOLD,d.GPT_6_LONG_CONTEXT_INPUT_THRESHOLD,d.GROK_4_5_LONG_CONTEXT_INPUT_THRESHOLD]
    sizes=[None,*sorted({0,300000,*[t+offset for t in thresholds for offset in (-1,0,1)]})]
    combos=itertools.product(models,dates,sizes,['','fast','priority','ultrafast','standard'])
    for model,date,size,tier in combos:
        case('price-boundary',{'op':'price','model':model,'timestamp':date,'input_tokens':size,'service_tier':tier},d.price_entry_for_model(model,date,size,tier))
    price_count=len(inputs)
    date_syntax=['2026-10-01','20261001','2026-W40','2026W40','2026-W40-4','2026W404','2026-02-30']
    clocks=['12','1230','123001','12:30','12:30:01','12:30:01.123456789','12:30:01,123','12.5','12:30.5','24:00:00','24:01','23:59:60']
    zones=['','Z','+08','-0430','+08:30','+08:00:30.5','+00:00:00.5','+08:60','+24:00']
    for date,separator,clock,zone in itertools.product(date_syntax,['T',' ','X','💡'],clocks,zones):
        value=date+separator+clock+zone
        parsed=d.parse_timestamp(value)
        case('timestamp-syntax',{'op':'timestamp','timestamp':value},d.utc_iso(parsed) if parsed else None)
    for date in date_syntax:
        parsed=d.parse_timestamp(date)
        case('timestamp-date',{'op':'timestamp','timestamp':date},d.utc_iso(parsed) if parsed else None)
    for trial in range(1000):
        model=rng.choice(models);date=rng.choice(dates);tier=rng.choice(['','priority','ultrafast'])
        usage={k:rng.randrange(-50,500_000) for k in d.TOKEN_KEYS}
        case('cost-rounding',{'op':'cost','model':model,'timestamp':date,'service_tier':tier,'usage':usage},d.estimate_cost_usd(usage,model,date,tier))
    for trial in range(60):
        timeline=[]
        for i in range(rng.randrange(0,100)):
            vals={k:rng.randrange(-50,5000) for k in d.TOKEN_KEYS}
            if i%9==0:vals.pop('cache_write_tokens');vals['cache_write_input_tokens']=7.9
            if i%7==0:vals['output_tokens']=None
            if i%11==0:vals['cached_input_tokens']='untrusted'
            if i%13==0:vals['reasoning_output_tokens']=True
            row={'timestamp':d.utc_iso(dt.datetime(2026,10,1,tzinfo=dt.UTC)+dt.timedelta(minutes=i)),'total_token_usage':vals,'model':rng.choice(models),'service_tier':rng.choice(['','priority','ultrafast'])}
            if i%3:row['last_token_usage']={k:1 for k in d.TOKEN_KEYS}
            timeline.append(row)
        if trial%2:rng.shuffle(timeline)
        if timeline:timeline.append({'timestamp':'invalid','total_token_usage':{}})
        normalized=[{'source_order':next(i for i,r in enumerate(timeline) if r is row),'timestamp':d.utc_iso(moment),'cumulative':cum,'delta':delta} for row,moment,cum,delta in d.timeline_rows_with_deltas(timeline)]
        case('timeline-fuzz',{'op':'timeline','timeline':timeline},normalized)
        case('price-segments-fuzz',{'op':'pricing','timeline':timeline,'model':'gpt-5'},d.pricing_for_timeline(timeline,'gpt-5'))
    for fixture in sorted((ROOT/'tests/fixtures/compatibility').glob('*.json')):
        value=json.loads(fixture.read_text())['expected']
        details=value.get('details_by_uid',{}).values() if isinstance(value,dict) else value[1:]
        for detail in details:
            case('golden-price-segments:'+fixture.name,{'op':'pricing','timeline':detail['timeline'],'model':detail['model'],'usage':detail['total_token_usage'],'timestamp':detail['end_at'],'service_tier':detail['service_tier']},d.pricing_for_timeline(detail['timeline'],detail['model'],detail['total_token_usage'],detail['end_at'],detail['service_tier']))
    for value in [None,[],{}, {'cache_write_tokens':3,'cache_write_input_tokens':8}, {'input_tokens':True,'cached_input_tokens':'3','total_tokens':4.9}]:
        case('usage-types',{'op':'normalize_usage','usage':value},d.normalize_usage(value))
    with tempfile.TemporaryDirectory(prefix='rust-reader-contract-') as temp:
        ignored={'timestamp':'2026-10-01T00:00:00Z','type':'response_item','payload':{'type':'function_call_output','output':'x'*200_000}}
        contents=[b'\xef\xbb\xbf{"text":"ok"}\n{"text":"\xff"}\n{broken}\nnull\n[]\n\n{"partial":',
                  b'{"complete":true}',b' \t',b'{"object":1}\n \t',
                  (json.dumps(ignored,separators=(',',':'))+'\n'+json.dumps(ignored)+'\n'+json.dumps({'payload':ignored['payload'],'timestamp':ignored['timestamp'],'type':'response_item'})+'\n').encode()]
        for i,content in enumerate(contents):
            path=Path(temp)/f'{i}.jsonl';path.write_bytes(content)
            for start,end in [(0,None),(0,len(content)//2),(len(content)//2,len(content)),(0,0)]:
                stats=d.RolloutReadStats();rows=list(d.read_rollout_jsonl(path,start,stats,end_offset=end))
                case('reader',{'op':'reader','path':str(path),'start':start,'end':end},{'rows':rows,'stats':{'bytes_read':stats.bytes_read,'fast_skipped_line_count':stats.fast_skipped_line_count,'fast_skipped_bytes':stats.fast_skipped_bytes}})
        for fixture in sorted((ROOT/'tests/fixtures/compatibility').glob('*.json')):
            contract=json.loads(fixture.read_text())
            sources=[]
            for i,(source,files) in enumerate(zip(contract['sources'],contract['files'])):
                home=Path(temp)/fixture.stem/str(i);home.mkdir(parents=True)
                sources.append(d.CodexLogSource(source['id'],source['label'],home))
                analyzer=d.CodexUsageAnalyzer([sources[-1]],resolve_project_info=False)
                for entry in files:
                    path=home/entry['path'];path.parent.mkdir(parents=True,exist_ok=True)
                    if 'sqlite_sql' in entry:
                        with sqlite3.connect(path) as db:db.executescript(entry['sqlite_sql'])
                        continue
                    path.write_bytes(base64.b64decode(entry['bytes_base64']));os.utime(path,ns=(entry['mtime_ns'],entry['mtime_ns']))
                    kind='archived' if 'archived_sessions' in path.parts else 'active'
                    _,detail=analyzer.parse_file(path,kind)
                    case('parse-file:'+fixture.name,{'op':'parse_file','path':str(path),'home':str(home),'source_id':source['id'],'source_label':source['label'],'kind':kind},{k:v for k,v in detail.items() if not k.startswith('_')})
                analyzer.close()
            if contract['operation']=='scan':
                analyzer=d.CodexUsageAnalyzer(sources,resolve_project_info=False)
                inventory=analyzer.iter_session_files()
                snapshot=analyzer.scan('all',include_remotes=False)
                compare(contract['expected'],canonical(snapshot,[s.codex_home for s in sources],contract['sources'],contract['files']))
                case('normalized-scan:'+fixture.name,{'op':'normalize_files','with_summary':True,'files':[{'path':str(path),'kind':kind,'home':str(source.codex_home),'source_id':source.id,'source_label':source.label} for source,path,kind in inventory]},{k:snapshot[k] for k in ['sessions','summary','daily_usage','details_by_uid']})
                case('indexed-scan:'+fixture.name,{'op':'indexed_scan','sources':[{'id':source.id,'label':source.label,'home':str(source.codex_home)} for source in sources],'database':str(Path(temp)/fixture.stem/'index.sqlite3')},{k:snapshot[k] for k in ['sessions','summary','daily_usage','details_by_uid']})
                for detail in snapshot['details_by_uid'].values():
                    start=d.parse_timestamp(detail['start_at']);end=d.parse_timestamp(detail['end_at'])
                    if start is not None and end is not None:
                        for first,last in [(start,end),(start+dt.timedelta(seconds=1),end),(end+dt.timedelta(seconds=1),end+dt.timedelta(days=1))]:
                            case('period-detail:'+fixture.name,{'op':'period_detail','detail':detail,'start':d.utc_iso(first),'end':d.utc_iso(last)},analyzer.detail_for_period(detail,first,last))
                analyzer.close()
        result=subprocess.run([str(ROOT/'target/release/examples/contract')],input='\n'.join(json.dumps(v) for v in inputs)+'\n',text=True,capture_output=True,check=True)
        actual=[json.loads(line) for line in result.stdout.splitlines()]
        assert len(actual)==len(expected),(len(actual),len(expected),result.stderr)
        failures=[]
        for i,(a,b) in enumerate(zip(expected,actual)):
            try:compare(a,b)
            except AssertionError as exc:
                failures.append({'case':i,'name':names[i],'error':str(exc),'input':inputs[i] if names[i] not in ['reader'] else {'op':'reader','case':i}})
        report={'status':'failed' if failures else 'passed','checks':len(expected),'price_grid_cases':price_count,'seed':64321,'comparison':'all public keys and types; exact integers and six-decimal floats','platform':platform.platform(),'rust':subprocess.check_output(['rustc','--version'],text=True).strip(),'source_sha256':hashlib.sha256(__import__('reference_dashboard').SOURCE.read_bytes()).hexdigest(),'failures':failures[:15], 'scope':'usage, normalized timeline, price lookup, rounding, price segments, streamed JSONL; raw session parser included; cross-file continuation, parent normalization and runtime tier merging included'}
        path=ROOT/'benchmarks/implementation/results/core-correctness.json';path.parent.mkdir(parents=True,exist_ok=True);path.write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({k:v for k,v in report.items() if k!='failures'},indent=2))
        if failures:
            print(json.dumps(failures[:3],indent=2)[:5000]);raise SystemExit(f'{len(failures)} failed cases')


if __name__=='__main__': main()
