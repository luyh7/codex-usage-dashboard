"""Freeze synthetic legacy tests once; replay JSON contracts without those tests.

Usage: python3 tools/compatibility.py [freeze|check]
Freeze refuses a changed production source. Never reads the user's log roots.
"""
from __future__ import annotations

import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
from reference_dashboard import SOURCE
FIXTURES = ROOT / 'tests/fixtures/compatibility'
BASELINE_SHA256 = '20fae68140fa8186ed86728aaac16d8d6688b1218d555e93e1a49e4761319135'


def load_dashboard():
    spec = importlib.util.spec_from_file_location('compat_dashboard', SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def canonical(value, homes, sources, files):
    # Rewrite only identities derived from an absolute file location. Continued
    # session UIDs are independent of location and are compared verbatim.
    identities = {}
    for i, home in enumerate(homes):
        for file in files[i]:
            relative = file['path']
            old = hashlib.sha1(f'{sources[i]["id"]}:{home / relative}'.encode()).hexdigest()[:16]
            new = hashlib.sha1(f'{sources[i]["id"]}:<HOME:{i}>/{relative}'.encode()).hexdigest()[:16]
            identities[old] = new

    def walk(item, location=()):
        if isinstance(item, dict):
            return {identities.get(k, k): walk(v, (*location, k)) for k, v in item.items()}
        if isinstance(item, list):
            return [walk(v, (*location, str(i))) for i, v in enumerate(item)]
        if isinstance(item, str):
            if location in [('generated_at',), ('period', 'end_at')]:
                if location == ('generated_at',) or value.get('period', {}).get('key') == 'all':
                    assert item and 'T' in item
                    return '<GENERATED_AT>'
            if location == ('snapshot_token',):
                assert len(item) >= 16
                return '<REVISION>'
            if item in identities:
                return identities[item]
            for i, home in enumerate(homes):
                if item == str(home) or item.startswith(str(home) + '/'):
                    return item.replace(str(home), f'<HOME:{i}>', 1)
        return item
    return walk(value)


def capture(analyzer, operation, args, kwargs, invoke):
    homes = [s.codex_home.resolve() for s in analyzer.codex_sources]
    sources = [{'id': s.id, 'label': s.label} for s in analyzer.codex_sources]
    files = []
    for home in homes:
        entries = []
        for path in sorted(home.rglob('*')):
            if not path.is_file() or path.suffix not in ('.jsonl', '.sqlite'):
                continue
            entry = {'path': str(path.relative_to(home)), 'mtime_ns': path.stat().st_mtime_ns}
            if path.name == 'logs_2.sqlite':
                with sqlite3.connect(path.as_uri() + '?mode=ro', uri=True) as db:
                    entry['sqlite_sql'] = '\n'.join(db.iterdump())
            elif path.suffix == '.jsonl':
                entry['bytes_base64'] = base64.b64encode(path.read_bytes()).decode()
            else:
                continue
            entries.append(entry)
        files.append(entries)
    call_args = list(args)
    if operation == 'parse_file':
        call_args = [str(Path(args[0]).resolve().relative_to(homes[0])), args[1]]
    result = invoke()
    public = result if operation == 'scan' else [{k: v for k, v in row.items() if not k.startswith('_')} for row in result]
    return result, {'operation': operation, 'sources': sources, 'files': files,
                    'args': call_args, 'kwargs': kwargs,
                    'expected': canonical(public, homes, sources, files)}


def compare(expected, actual, path='$'):
    if isinstance(expected, dict):
        assert isinstance(actual, dict), f'{path}: expected object'
        assert expected.keys() == actual.keys(), f'{path}: keys {expected.keys() ^ actual.keys()}'
        for key in expected:
            compare(expected[key], actual[key], f'{path}.{key}')
    elif isinstance(expected, list):
        assert isinstance(actual, list) and len(expected) == len(actual), f'{path}: array length'
        for i, (a, b) in enumerate(zip(expected, actual)):
            compare(a, b, f'{path}[{i}]')
    else:
        # Six-decimal money is exact, including null/unknown and booleans.
        assert type(expected) is type(actual) and expected == actual, f'{path}: {expected!r} != {actual!r}'


def replay(case, dashboard=None):
    dashboard = dashboard or load_dashboard()
    with tempfile.TemporaryDirectory(prefix='dashboard-contract-') as tmp:
        homes = [Path(tmp) / str(i) for i in range(len(case['sources']))]
        for home, files in zip(homes, case['files']):
            home.mkdir()
            for entry in files:
                path = home / entry['path']
                path.parent.mkdir(parents=True, exist_ok=True)
                if 'sqlite_sql' in entry:
                    with sqlite3.connect(path) as db:
                        db.executescript(entry['sqlite_sql'])
                else:
                    path.write_bytes(base64.b64decode(entry['bytes_base64']))
                os.utime(path, ns=(entry['mtime_ns'], entry['mtime_ns']))
        sources = [dashboard.CodexLogSource(s['id'], s['label'], home) for s, home in zip(case['sources'], homes)]
        analyzer = dashboard.CodexUsageAnalyzer(sources, resolve_project_info=False)
        try:
            args = list(case['args'])
            if case['operation'] == 'parse_file':
                args[0] = homes[0] / args[0]
            actual = getattr(analyzer, case['operation'])(*args, **case['kwargs'])
            if case['operation'] == 'parse_file':
                actual = [{k: v for k, v in row.items() if not k.startswith('_')} for row in actual]
            actual = canonical(actual, homes, case['sources'], case['files'])
            compare(case['expected'], actual)
            return actual
        finally:
            analyzer.close()


def freeze():
    assert hashlib.sha256(SOURCE.read_bytes()).hexdigest() == BASELINE_SHA256, 'Freeze only from the audited legacy source'
    sys.path.insert(0, str(ROOT / 'tests'))
    import test_codex_usage_dashboard as legacy
    selected = [
        'test_scan_merges_continued_rollout_files_for_the_same_session',
        'test_subagent_usage_excludes_inherited_parent_timeline_prefix',
        'test_subagent_inherits_parent_service_tier_at_spawn',
        'test_subagent_before_parent_first_token_resolves_zero_baseline',
        'test_subagent_last_n_slice_with_counter_reset_sums_event_deltas',
        'test_missing_fork_parent_keeps_branch_total_but_excludes_exact_usage',
        'test_runtime_log_tier_reprices_cached_rollouts',
        'test_parse_file_prices_service_tier_changes_per_token_event',
    ]
    cls = legacy.dashboard.CodexUsageAnalyzer
    original_scan, original_parse = cls.scan, cls.parse_file
    records = []
    busy = False
    current = ''
    def wrap(operation, original):
        def run(analyzer, *args, **kwargs):
            nonlocal busy
            if busy:
                return original(analyzer, *args, **kwargs)
            busy = True
            try:
                result, record = capture(analyzer, operation, args, kwargs, lambda: original(analyzer, *args, **kwargs))
                record['origin_test'] = current
                records.append(record)
                return result
            finally:
                busy = False
        return run
    with patch.object(cls, 'scan', wrap('scan', original_scan)), patch.object(cls, 'parse_file', wrap('parse_file', original_parse)):
        for current in selected:
            test = legacy.CodexUsageDashboardTests(current)
            result = unittest.TestResult()
            test.run(result)
            assert result.wasSuccessful(), result.errors + result.failures
    FIXTURES.mkdir(parents=True, exist_ok=True)
    for i, record in enumerate(records):
        replay(record)
        (FIXTURES / f'{i:02d}-{record["origin_test"][5:]}.json').write_text(json.dumps(record, ensure_ascii=False, indent=2) + '\n')
    print(f'Frozen and independently replayed {len(records)} cases')


if __name__ == '__main__':
    os.environ['TZ'] = 'UTC'
    if hasattr(time, 'tzset'):
        time.tzset()
    if sys.argv[1:] == ['freeze']:
        freeze()
    else:
        paths = sorted(FIXTURES.glob('*.json'))
        assert paths, 'No frozen contracts'
        for path in paths:
            replay(json.loads(path.read_text()))
        print(f'{len(paths)} compatibility cases passed')
