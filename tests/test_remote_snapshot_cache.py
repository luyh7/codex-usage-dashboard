import datetime as dt
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import test_codex_usage_dashboard as existing


dashboard = existing.dashboard


class RemoteSnapshotCacheTests(unittest.TestCase):
    @staticmethod
    def write_local_usage(
        codex_home: Path,
        session_id: str,
        total_tokens: int,
        timestamp: str,
    ) -> Path:
        return existing.CodexUsageDashboardTests().write_usage_file(
            codex_home,
            session_id,
            total_tokens,
            timestamp,
        )

    @staticmethod
    def remote_payload(
        device_code: str,
        label: str,
        session_id: str,
        title: str,
        total_tokens: int,
        timestamp: str = "2026-09-05T12:00:00Z",
    ) -> dict:
        uid = f"{session_id}-uid"
        usage = {"input_tokens": total_tokens, "total_tokens": total_tokens}
        session = {
            "uid": uid,
            "session_id": session_id,
            "title": title,
            "model": "gpt-5",
            "start_at": timestamp,
            "end_at": timestamp,
            "updated_at": timestamp,
            "total_token_usage": usage,
            "last_token_usage": usage,
            "estimated_cost_usd": 0.0,
            "estimated_cost_breakdown_usd": {},
            "price_model_known": True,
        }
        detail = {
            **session,
            "timeline": [
                {
                    "timestamp": timestamp,
                    "model": "gpt-5",
                    "total_token_usage": usage,
                    "last_token_usage": usage,
                }
            ],
        }
        return {
            "schema": dashboard.SNAPSHOT_SCHEMA,
            "version": dashboard.SNAPSHOT_VERSION,
            "device": {
                "short_code": device_code,
                "label": label,
                "platform": "macOS",
                "hostname": f"{device_code}-host",
            },
            "exported_at": timestamp,
            "imported_at": timestamp,
            "snapshot": {
                "generated_at": timestamp,
                "sessions": [session],
                "details_by_uid": {uid: detail},
                "summary": {"usage": usage},
                "daily_usage": [],
            },
        }

    @staticmethod
    def remote_session(snapshot: dict, session_id: str) -> dict:
        return next(row for row in snapshot["sessions"] if row["session_id"] == session_id)

    @staticmethod
    def write_external_snapshot(path: Path, payload: dict) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(payload, sort_keys=True), encoding="utf-8")

    @staticmethod
    def reprice_calls_at_timestamp(mock, timestamp: str) -> int:
        return sum(
            1
            for call in mock.call_args_list
            if call.args
            and isinstance(call.args[0], list)
            and call.args[0]
            and isinstance(call.args[0][0], dict)
            and call.args[0][0].get("timestamp") == timestamp
        )

    def test_local_rebuild_and_list_remotes_reuse_unchanged_remote_payload(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            local_home = root / "local" / ".codex"
            self.write_local_usage(local_home, "local-first", 100, "2026-09-05T10:00:00Z")
            store = dashboard.RemoteSnapshotStore("local-device", root / "remotes")
            self.assertTrue(
                store.import_snapshot(
                    self.remote_payload(
                        "remote-device",
                        "Remote Device",
                        "remote-session",
                        "Remote session",
                        200,
                    ),
                    label="Remote Device",
                )["ok"]
            )
            analyzer = dashboard.CodexUsageAnalyzer(
                [dashboard.CodexLogSource("local", "Local", local_home)],
                remote_store=store,
                resolve_project_info=False,
            )

            with patch.object(
                dashboard,
                "read_json_file",
                wraps=dashboard.read_json_file,
            ) as read_json, patch.object(
                dashboard,
                "pricing_for_timeline",
                wraps=dashboard.pricing_for_timeline,
            ) as reprice:
                first = analyzer.scan()
                reads_after_prime = read_json.call_count
                remote_reprices_after_prime = self.reprice_calls_at_timestamp(
                    reprice, "2026-09-05T12:00:00Z"
                )
                self.assertGreater(reads_after_prime, 0)
                self.assertGreater(remote_reprices_after_prime, 0)

                listed = store.list_remotes()
                self.assertEqual(read_json.call_count, reads_after_prime)
                self.assertEqual(listed[0]["usage"]["total_tokens"], 200)

                self.write_local_usage(local_home, "local-second", 50, "2026-09-05T10:05:00Z")
                rebuilt = analyzer.scan()

            self.assertEqual(read_json.call_count, reads_after_prime)
            self.assertEqual(
                self.reprice_calls_at_timestamp(reprice, "2026-09-05T12:00:00Z"),
                remote_reprices_after_prime,
            )
            self.assertEqual(rebuilt["summary"]["session_count"], 3)
            self.assertEqual(rebuilt["summary"]["usage"]["total_tokens"], 350)
            self.assertEqual(first["summary"]["usage"]["total_tokens"], 300)

    def test_reimporting_one_device_only_reprices_that_device(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            store = dashboard.RemoteSnapshotStore("local-device", root / "remotes")
            first_device = self.remote_payload(
                "device-one", "Device One", "one-session", "One original", 100
            )
            second_device = self.remote_payload(
                "device-two", "Device Two", "two-session", "Two original", 300
            )
            self.assertTrue(store.import_snapshot(first_device, label="Device One")["ok"])
            self.assertTrue(store.import_snapshot(second_device, label="Device Two")["ok"])
            analyzer = dashboard.CodexUsageAnalyzer(
                root / "local" / ".codex",
                remote_store=store,
                resolve_project_info=False,
            )

            with patch.object(
                dashboard,
                "pricing_for_timeline",
                wraps=dashboard.pricing_for_timeline,
            ) as reprice:
                first = analyzer.scan()
                reprices_after_prime = reprice.call_count
                self.assertEqual(reprices_after_prime, 2)

                updated_first_device = self.remote_payload(
                    "device-one", "Device One", "one-session", "One updated", 260
                )
                self.assertTrue(store.import_snapshot(updated_first_device)["ok"])
                rebuilt = analyzer.scan()

            self.assertEqual(reprice.call_count, reprices_after_prime + 1)
            self.assertEqual(first["summary"]["usage"]["total_tokens"], 400)
            self.assertEqual(rebuilt["summary"]["usage"]["total_tokens"], 560)
            self.assertEqual(self.remote_session(rebuilt, "one-session")["title"], "One updated")
            self.assertEqual(self.remote_session(rebuilt, "two-session")["title"], "Two original")

    def test_rename_delete_and_reimport_invalidate_cached_results_and_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            store = dashboard.RemoteSnapshotStore("local-device", root / "remotes")
            original = self.remote_payload(
                "remote-device", "Original name", "remote-session", "Original title", 100
            )
            self.assertTrue(store.import_snapshot(original, label="Original name")["ok"])
            analyzer = dashboard.CodexUsageAnalyzer(
                root / "local" / ".codex",
                remote_store=store,
                resolve_project_info=False,
            )
            analyzer.scan()

            store.rename_remote("remote-device", "Renamed device")
            renamed = analyzer.scan()
            self.assertEqual(self.remote_session(renamed, "remote-session")["environment"], "Renamed device")
            self.assertEqual(store.list_remotes()[0]["label"], "Renamed device")

            store.delete_remote("remote-device")
            deleted = analyzer.scan()
            self.assertEqual(deleted["summary"]["session_count"], 0)
            self.assertEqual(store.list_remotes(), [])

            reimported = self.remote_payload(
                "remote-device", "Reimported device", "remote-session", "Reimported title", 250
            )
            self.assertTrue(store.import_snapshot(reimported, label="Reimported device")["ok"])
            restored = analyzer.scan()

        row = self.remote_session(restored, "remote-session")
        self.assertEqual(row["title"], "Reimported title")
        self.assertEqual(row["environment"], "Reimported device")
        self.assertEqual(row["total_token_usage"]["total_tokens"], 250)

    def test_atomic_replacement_with_same_mtime_and_size_invalidates_cached_snapshot(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            store = dashboard.RemoteSnapshotStore("local-device", root / "remotes")
            path = store.snapshot_path("remote-device")
            original = self.remote_payload(
                "remote-device", "Remote device", "remote-session", "Original", 111
            )
            replacement = self.remote_payload(
                "remote-device", "Remote device", "remote-session", "Replaced", 222
            )
            self.write_external_snapshot(path, original)
            analyzer = dashboard.CodexUsageAnalyzer(
                root / "local" / ".codex",
                remote_store=store,
                resolve_project_info=False,
            )
            first = analyzer.scan()
            original_state = path.stat()

            replacement_path = path.with_suffix(".replacement")
            self.write_external_snapshot(replacement_path, replacement)
            self.assertEqual(replacement_path.stat().st_size, original_state.st_size)
            os.utime(
                replacement_path,
                ns=(original_state.st_atime_ns, original_state.st_mtime_ns),
            )
            os.replace(replacement_path, path)
            replacement_state = path.stat()
            self.assertEqual(replacement_state.st_size, original_state.st_size)
            self.assertEqual(replacement_state.st_mtime_ns, original_state.st_mtime_ns)
            self.assertNotEqual(replacement_state.st_ino, original_state.st_ino)

            rebuilt = analyzer.scan()

        self.assertEqual(self.remote_session(first, "remote-session")["title"], "Original")
        row = self.remote_session(rebuilt, "remote-session")
        self.assertEqual(row["title"], "Replaced")
        self.assertEqual(row["total_token_usage"]["total_tokens"], 222)

    def test_published_snapshot_is_unchanged_by_rename_reimport_and_new_period_scan(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            timestamp = dashboard.utc_iso(dt.datetime.now(dt.UTC))
            store = dashboard.RemoteSnapshotStore("local-device", root / "remotes")
            original = self.remote_payload(
                "remote-device", "Original device", "remote-session", "Original title", 100, timestamp
            )
            self.assertTrue(store.import_snapshot(original, label="Original device")["ok"])
            analyzer = dashboard.CodexUsageAnalyzer(
                root / "local" / ".codex",
                remote_store=store,
                resolve_project_info=False,
            )
            held = analyzer.scan()

            store.rename_remote("remote-device", "Renamed device")
            renamed = analyzer.scan()
            self.assertEqual(self.remote_session(renamed, "remote-session")["environment"], "Renamed device")

            replacement = self.remote_payload(
                "remote-device", "Replacement device", "remote-session", "Replacement title", 250, timestamp
            )
            self.assertTrue(store.import_snapshot(replacement, label="Replacement device")["ok"])
            period = analyzer.scan("today")

        held_row = self.remote_session(held, "remote-session")
        self.assertEqual(held_row["environment"], "Original device")
        self.assertEqual(held_row["title"], "Original title")
        self.assertEqual(held_row["total_token_usage"]["total_tokens"], 100)
        current_row = self.remote_session(period, "remote-session")
        self.assertEqual(current_row["environment"], "Replacement device")
        self.assertEqual(current_row["title"], "Replacement title")
        self.assertEqual(current_row["total_token_usage"]["total_tokens"], 250)

    def test_mutating_published_nested_data_does_not_poison_remote_cache(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            store = dashboard.RemoteSnapshotStore("local-device", root / "remotes")
            store.import_snapshot(
                self.remote_payload("remote-device", "Remote", "remote-session", "Original", 100),
                label="Remote",
            )
            local_home = root / "local" / ".codex"
            analyzer = dashboard.CodexUsageAnalyzer(
                local_home, remote_store=store, resolve_project_info=False,
            )
            self.addCleanup(analyzer.close)
            first = analyzer.scan("all")
            row = self.remote_session(first, "remote-session")
            uid = row["uid"]
            row["total_token_usage"]["total_tokens"] = 999
            first["details_by_uid"][uid]["timeline"][0]["total_token_usage"]["total_tokens"] = 999

            self.write_local_usage(local_home, "local-session", 50, "2026-09-05T10:00:00Z")
            rebuilt = analyzer.scan("all")
            restored = self.remote_session(rebuilt, "remote-session")
            self.assertEqual(restored["total_token_usage"]["total_tokens"], 100)
            self.assertEqual(
                rebuilt["details_by_uid"][uid]["timeline"][0]["total_token_usage"]["total_tokens"],
                100,
            )

    def test_mutating_read_all_result_does_not_change_cached_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            store = dashboard.RemoteSnapshotStore("local-device", Path(temp_dir))
            store.import_snapshot(
                self.remote_payload("remote-device", "Original", "remote-session", "Title", 100),
                label="Original",
            )
            payload = store.read_all()[0]
            payload["device"]["label"] = "Mutated"
            payload["snapshot"]["sessions"][0]["total_token_usage"]["total_tokens"] = 999
            self.assertEqual(store.list_remotes()[0]["label"], "Original")
            self.assertEqual(
                store.read_all()[0]["snapshot"]["sessions"][0]["total_token_usage"]["total_tokens"],
                100,
            )

    def test_corrupted_snapshot_is_retried_after_repair(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            store = dashboard.RemoteSnapshotStore("local-device", root / "remotes")
            path = store.snapshot_path("remote-device")
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("{corrupted", encoding="utf-8")
            analyzer = dashboard.CodexUsageAnalyzer(
                root / "local" / ".codex",
                remote_store=store,
                resolve_project_info=False,
            )

            first = analyzer.scan()
            self.assertEqual(first["summary"]["session_count"], 0)
            self.assertEqual(store.list_remotes(), [])

            self.write_external_snapshot(
                path,
                self.remote_payload(
                    "remote-device", "Repaired device", "remote-session", "Repaired title", 180
                ),
            )
            repaired = analyzer.scan()

        row = self.remote_session(repaired, "remote-session")
        self.assertEqual(row["title"], "Repaired title")
        self.assertEqual(row["total_token_usage"]["total_tokens"], 180)


if __name__ == "__main__":
    unittest.main()
