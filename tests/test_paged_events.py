"""Revision-bound query contracts; fixtures never access the user's Codex home."""
import copy
import datetime as dt
import http.client
import json
from pathlib import Path
import tempfile
import threading
import unittest
from urllib.parse import urlencode
from unittest.mock import patch

from test_codex_usage_dashboard import dashboard


class PagedEventsTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.analyzer = dashboard.CodexUsageAnalyzer(Path(self.tmp.name))
        self.addCleanup(self.analyzer.close)
        start = dt.datetime(2026, 1, 1, tzinfo=dt.UTC)
        # Duplicate timestamps, out-of-order events and an invalid timestamp:
        # equal timestamps retain source order; nothing is deduplicated.
        self.events = [{'timestamp': (start + dt.timedelta(seconds=i % 97)).isoformat(),
                        'event_id': i, 'total_token_usage': {'total_tokens': i},
                        'last_token_usage': {'total_tokens': 1}}
                       for i in range(10003)]
        self.events[300]['timestamp'] = 'invalid'
        self.snapshot = self.analyzer.publish_snapshot({'details_by_uid': {
            'session-a': {'uid': 'session-a', 'timeline': self.events, 'token_event_count': len(self.events)},
            'empty': {'uid': 'empty', 'timeline': []},
        }})
        self.token = self.snapshot['snapshot_token']

    def test_every_page_preserves_all_events_and_order(self):
        before = copy.deepcopy(self.events)
        cursor = ''
        actual = []
        previous = None
        while True:
            page = self.analyzer.query_events('session-a', self.token, cursor)
            self.assertLessEqual(len(page['events']), 100)
            self.assertEqual(page['offset'], len(actual))
            self.assertEqual(page['total'], len(self.events))
            if previous is not None:
                back = self.analyzer.query_events('session-a', self.token, page['previous_cursor'])
                self.assertEqual(back['events'], previous['events'])
            actual.extend(page['events'])
            previous = page
            if page['next_cursor'] is None:
                break
            cursor = page['next_cursor']
        expected = sorted(before, key=lambda row: dashboard.parse_timestamp(row['timestamp']) or dt.datetime.min.replace(tzinfo=dt.UTC), reverse=True)
        self.assertEqual(actual, expected)
        self.assertEqual(len({row['event_id'] for row in actual}), 10003)
        self.assertEqual(self.events, before)
        self.assertEqual(len(self.analyzer._event_order_cache), 1)

    def test_cursor_binding_empty_and_invalid_queries(self):
        page = self.analyzer.query_events('session-a', self.token)
        for uid, token, cursor, limit in [
            ('empty', self.token, page['next_cursor'], 100),
            ('session-a', 'other', page['next_cursor'], 100),
            ('session-a', self.token, page['next_cursor'], 50),
            ('session-a', self.token, 'not-json', 100),
            ('session-a', self.token, 'e30=', 100),
            ('session-a', self.token, '', 101),
            ('session-a', '', '', 100),
        ]:
            with self.subTest(uid=uid, token=token, cursor=cursor, limit=limit):
                with self.assertRaises(ValueError):
                    self.analyzer.query_events(uid, token, cursor, limit)
        empty = self.analyzer.query_events('empty', self.token)
        self.assertEqual(empty['events'], [])
        self.assertIsNone(empty['next_cursor'])
        self.assertIsNone(empty['previous_cursor'])

    def test_snapshot_retirement_rejects_cursor_and_reclaims_order(self):
        page = self.analyzer.query_events('session-a', self.token)
        self.analyzer.unpublish_snapshot(self.snapshot)
        self.assertEqual(self.analyzer._event_order_cache, {})
        with self.assertRaises(dashboard.SnapshotStaleError):
            self.analyzer.query_events('session-a', self.token, page['next_cursor'])

    def test_http_queries_are_read_only_and_legacy_detail_stays_complete(self):
        server = dashboard.ThreadingHTTPServer(('127.0.0.1', 0), dashboard.make_handler(self.analyzer))
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        def get(path, **query):
            conn = http.client.HTTPConnection(*server.server_address, timeout=5)
            try:
                conn.request('GET', path + '?' + urlencode(query))
                response = conn.getresponse()
                return response.status, json.loads(response.read())
            finally:
                conn.close()
        params = {'id': 'session-a', 'snapshot_token': self.token}
        with patch.object(self.analyzer, 'scan', side_effect=AssertionError('query must not scan')):
            status, legacy = get('/api/session', **params)
            self.assertEqual(status, 200)
            self.assertEqual(len(legacy['timeline']), 10003)
            status, detail = get('/api/v2/session', **params)
            self.assertEqual(status, 200)
            self.assertNotIn('timeline', detail)
            status, chart = get('/api/v2/chart', **params)
            self.assertEqual(status, 200)
            self.assertEqual([p['total_token_usage']['total_tokens'] for p in chart['timeline']], list(range(10003)))
            status, page = get('/api/v2/events', **params)
            self.assertEqual((status, len(page['events'])), (200, 100))
            self.assertEqual(get('/api/v2/events', **params, limit='bad')[0], 400)
            self.assertEqual(get('/api/v2/events', id='session-a')[0], 400)
            self.assertEqual(get('/api/v2/session', id='absent', snapshot_token=self.token)[0], 404)
            self.assertEqual(get('/api/v2/chart', id='session-a', snapshot_token='stale')[0], 409)


if __name__ == '__main__':
    unittest.main()
