"""Differential edge/fuzz checks plus hand-calculable accounting assertions."""
import datetime as dt
import itertools
import json
import random
import subprocess
from backend import d, WORK, HERE, dump, equal, operation, config, u, timestamp

def main():
    rng=random.Random(64321);checks=[]
    exe=HERE/'rust/target/release/dashboard-refactor-probe'
    def check(name,c,expected=None):
        path=WORK/'correctness.json';dump(path,c)
        py=operation(c)()
        rust=json.loads(subprocess.check_output([str(exe),str(path),'1'],text=True))['result']
        equal(py,rust)
        if expected is not None:equal(expected,py)
        checks.append(name)
    t=[{'timestamp':timestamp(i),'total_token_usage':{'input_tokens':x,'total_tokens':x},'last_token_usage':{'input_tokens':y,'total_tokens':y}} for i,(x,y) in enumerate([(100,100),(160,60),(20,20)])]
    case={'op':'fork','cases':[{'parent':t[:1],'child':t}]}
    check('hand_fork_reset',case)
    result=operation(case)()[0]
    assert result['prefix']==1 and result['total']['total_tokens']==80 and result['last']['total_tokens']==20
    cost={'op':'cost_typed','cases':[{'usage':{'input_tokens':1000000,'cached_input_tokens':600000,'output_tokens':100000,'reasoning_output_tokens':30000},'prices':{'input':2.5,'cached_input':.25,'output':15}}]}
    check('hand_cost_no_reasoning_double_charge',cost,[{'input_tokens':1.,'cached_input_tokens':.15,'cache_write_tokens':0.,'output_tokens':1.5,'reasoning_output_tokens':.45}])
    assert d.total_cost_from_parts(operation(cost)()[0])==2.65
    dates=[None,'invalid','2026-01-01T00:00:00Z']
    for t,_ in d.MODEL_PRICE_SCHEDULES:
        if t:dates.extend(d.utc_iso(t+dt.timedelta(seconds=x)) for x in [-1,0,1])
    models=list(d.MODEL_PRICES_USD_PER_M_TOKENS)+['unknown','router/gpt-5.6-sol','gpt-daybreak-blue-latest','gpt-daybreak-red-latest']
    thresholds=[d.GPT_5_6_LONG_CONTEXT_INPUT_THRESHOLD,d.GPT_6_LONG_CONTEXT_INPUT_THRESHOLD,d.GROK_4_5_LONG_CONTEXT_INPUT_THRESHOLD]
    sizes=[None,*sorted({0,300000,*[t+offset for t in thresholds for offset in (-1,0,1)]})]
    combos=list(itertools.product(models,dates,sizes,['','fast','priority','ultrafast','standard']))
    check('exhaustive_price_boundary_grid',{'op':'pricing','config':config(),'cases':[dict(zip(['model','timestamp','input_tokens','service_tier'],c)) for c in combos]})
    for trial in range(60):
        rows=[]
        for i in range(rng.randrange(0,100)):
            vals={k:rng.randrange(-50,5000) for k in d.TOKEN_KEYS}
            if i%9==0:vals.pop('cache_write_tokens');vals['cache_write_input_tokens']=7.9
            if i%7==0:vals['output_tokens']=None
            if i%11==0:vals['cached_input_tokens']='untrusted'
            if i%13==0:vals['reasoning_output_tokens']=True
            row={'timestamp':timestamp(i),'total_token_usage':vals}
            if i%3:row['last_token_usage']=u(1)
            rows.append(row)
        if trial%2:rng.shuffle(rows)
        if rows:rows.append({'timestamp':'invalid','total_token_usage':u(10)})
        check(f'timeline_fuzz_{trial}',{'op':'timeline_typed','timeline':rows})
    for trial in range(20):
        path=WORK/f'edge-reader-{trial}.jsonl'
        ignored={'timestamp':'2026-10-01T00:00:00Z','type':'response_item','payload':{'type':'function_call_output','output':'x'*8192}}
        lines=[json.dumps(ignored,separators=(',',':')),json.dumps(ignored),json.dumps({'payload':ignored['payload'],'timestamp':ignored['timestamp'],'type':'response_item'},separators=(',',':')),'{broken}', 'null','[]','',json.dumps({'text':'中文'})]
        raw=('\n'.join(lines)+'\n').encode();raw=(b'\xef\xbb\xbf' if trial%2 else b'')+raw
        raw += b'{"partial":' if trial%3 else b'{"complete":true}'
        path.write_bytes(raw)
        check(f'reader_layout_{trial}',{'op':'reader','path':str(path),'return_rows':True})
    dump(HERE/'results/correctness.json',{'status':'passed','seed':64321,'checks':checks,'price_grid_cases':len(combos),'integer_comparison':'exact','float_tolerance':{'relative':1e-12,'absolute':1e-8},
        'scope':'selected module contracts; production analyzer normalization/inheritance is still covered only by its Python tests, not fully ported',
        'limitations':['timestamp parser probe supports generated RFC3339 and naive ISO datetimes; not all Python fromisoformat inputs','Rust integer representation is signed 64-bit','daily probe timezone fixed to Asia/Shanghai; DST/platform validation belongs to migration gates']})
    print(f'{len(checks)} differential checks passed, including {len(combos)} pricing boundary cases.')

if __name__=='__main__':main()
