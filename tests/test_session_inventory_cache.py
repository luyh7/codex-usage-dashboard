import datetime as dt
import os
import tempfile
import unittest
from collections import Counter
from pathlib import Path
from unittest import mock

import test_codex_usage_dashboard as existing


dashboard = existing.dashboard


class SessionInventoryCacheTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.home = Path(self.temporary.name) / ".codex"
        self.fixtures = existing.CodexUsageDashboardTests()
        self.now = dashboard.utc_iso(dt.datetime.now(dt.UTC))
        self.old = dt.datetime.now(dt.UTC) - dt.timedelta(days=3)

    def write_session(self, session_id: str, tokens: int, old: bool = False) -> Path:
        path = self.fixtures.write_usage_file(
            self.home, session_id, tokens,
            dashboard.utc_iso(self.old) if old else self.now,
        )
        if old:
            os.utime(path, (self.old.timestamp(), self.old.timestamp()))
        return path

    def analyzer(self, **kwargs):
        analyzer = dashboard.CodexUsageAnalyzer(
            self.home, resolve_project_info=False, **kwargs,
        )
        self.addCleanup(analyzer.close)
        return analyzer

    def test_warm_bounded_scan_reuses_directory_listing_and_file_stats(self) -> None:
        old_paths = {self.write_session(f"old-{index}", 100, old=True) for index in range(3)}
        self.write_session("active", 250)
        analyzer = self.analyzer()
        first = analyzer.scan("today")
        stat_calls = Counter()
        original_stat = Path.stat

        def counted_stat(path, *args, **kwargs):
            stat_calls[path] += 1
            return original_stat(path, *args, **kwargs)

        with mock.patch.object(Path, "stat", counted_stat), mock.patch.object(
            os, "scandir", wraps=os.scandir,
        ) as scandir:
            second = analyzer.scan("today")

        self.assertEqual(second["snapshot_token"], first["snapshot_token"])
        self.assertEqual(scandir.call_count, 0)
        self.assertEqual({stat_calls[path] for path in old_paths}, {1})

    def test_cached_inventory_discovers_nested_files_and_archive_moves(self) -> None:
        first_path = self.write_session("first", 100)
        analyzer = self.analyzer()
        analyzer.scan("all")
        second_path = self.write_session("second", 250, old=True)
        nested = self.home / "sessions" / "2026" / "09" / "05"
        nested.mkdir(parents=True)
        second_path = second_path.rename(nested / second_path.name)
        archive = self.home / "archived_sessions"
        archive.mkdir()
        first_path.rename(archive / first_path.name)

        snapshot = analyzer.scan("all")
        sessions = {row["session_id"]: row for row in snapshot["sessions"]}
        self.assertEqual(set(sessions), {"first", "second"})
        self.assertEqual(sessions["first"]["source"], "archived")
        self.assertEqual(sessions["second"]["path"], str(second_path))

        second_path.unlink()
        self.assertEqual(
            [row["session_id"] for row in analyzer.scan("all")["sessions"]],
            ["first"],
        )

    def test_native_history_resume_is_visible_on_next_scan(self) -> None:
        self.write_session("resumed", 100, old=True)
        analyzer = self.analyzer()
        self.assertEqual(analyzer.scan("today")["sessions"], [])

        self.write_session("resumed", 300)
        snapshot = analyzer.scan("today")
        self.assertEqual(snapshot["summary"]["usage"]["total_tokens"], 300)

    def test_active_log_updates_even_when_clock_has_not_advanced(self) -> None:
        self.write_session("active", 100)
        analyzer = self.analyzer(inventory_refresh_seconds=2.0)
        with mock.patch.object(dashboard.time, "monotonic", return_value=100.0):
            analyzer.scan("today")
            self.write_session("active", 300)
            snapshot = analyzer.scan("today")
        self.assertEqual(snapshot["summary"]["usage"]["total_tokens"], 300)

    def test_inventory_preserves_active_before_archived_when_mtimes_tie(self) -> None:
        active = self.write_session("active", 100)
        archived = self.write_session("archived", 200)
        archive = self.home / "archived_sessions"
        archive.mkdir()
        archived = archived.rename(archive / archived.name)
        moment = active.stat().st_mtime_ns
        os.utime(archived, ns=(moment, moment))
        files = self.analyzer().iter_session_files()
        self.assertEqual([source for _log_source, _path, source in files], ["active", "archived"])

    def test_historical_stat_refresh_is_bounded_and_resumed_files_become_active(self) -> None:
        path = self.write_session("resumed", 100, old=True)
        analyzer = self.analyzer(inventory_refresh_seconds=2.0)
        original_stat = Path.stat
        stat_calls = Counter()

        def counted_stat(candidate, *args, **kwargs):
            stat_calls[candidate] += 1
            return original_stat(candidate, *args, **kwargs)

        with mock.patch.object(dashboard.time, "monotonic", return_value=100.0):
            first = analyzer.scan("all")
        with mock.patch.object(dashboard.time, "monotonic", return_value=101.0), mock.patch.object(
            Path, "stat", counted_stat,
        ):
            self.assertEqual(analyzer.scan("all")["snapshot_token"], first["snapshot_token"])
        self.assertEqual(stat_calls[path], 0)

        self.write_session("resumed", 300)
        with mock.patch.object(dashboard.time, "monotonic", return_value=102.1):
            self.assertEqual(analyzer.scan("today")["summary"]["usage"]["total_tokens"], 300)

        self.write_session("resumed", 500)
        with mock.patch.object(dashboard.time, "monotonic", return_value=102.2):
            self.assertEqual(analyzer.scan("today")["summary"]["usage"]["total_tokens"], 500)

    def test_directory_change_invalidates_historical_stats_before_deadline(self) -> None:
        path = self.write_session("replaced", 100, old=True)
        analyzer = self.analyzer(inventory_refresh_seconds=2.0)
        with mock.patch.object(dashboard.time, "monotonic", return_value=100.0):
            analyzer.scan("all")
        previous_stat = path.stat()
        replacement = self.write_session("replacement", 300, old=True)
        replacement.replace(path)
        os.utime(path, ns=(previous_stat.st_atime_ns, previous_stat.st_mtime_ns))

        with mock.patch.object(dashboard.time, "monotonic", return_value=100.1):
            snapshot = analyzer.scan("all")
        self.assertEqual(snapshot["sessions"][0]["session_id"], "replacement")
        self.assertEqual(snapshot["summary"]["usage"]["total_tokens"], 300)

    def test_dependency_index_observes_metadata_rewrite_with_preserved_mtime(self) -> None:
        first_parent = self.write_session("parent-one", 100, old=True)
        second_parent = self.write_session("parent-two", 200, old=True)

        def write_child(parent_id):
            return self.fixtures.write_rollout_rows(self.home, "child", [{
                "timestamp": self.now,
                "type": "session_meta",
                "payload": {"id": "child", "forked_from_id": parent_id},
            }])

        child = write_child("parent-one")
        analyzer = self.analyzer()

        def dependency_paths():
            files = analyzer.iter_session_files()
            candidates = [item for item in files if item[1] == child]
            return {item[1] for item in analyzer.files_with_fork_dependencies(candidates, files)}

        self.assertEqual(dependency_paths(), {child, first_parent})
        previous = child.stat()
        write_child("parent-two")
        os.utime(child, ns=(previous.st_atime_ns, previous.st_mtime_ns))
        if child.stat().st_ctime_ns == previous.st_ctime_ns:
            self.skipTest("File system does not expose this rewrite through ctime")
        self.assertEqual(dependency_paths(), {child, second_parent})


if __name__ == "__main__":
    unittest.main()
