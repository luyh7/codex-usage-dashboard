import datetime as dt
import http.client
import importlib.util
import json
import os
import shutil
import sqlite3
import subprocess
import tempfile
import threading
import time
import unittest
from pathlib import Path
from urllib.parse import urlencode


MODULE_PATH = (
    Path(__file__).resolve().parents[1]
    / "skills"
    / "codex-usage-dashboard"
    / "scripts"
    / "codex_usage_dashboard.py"
)

spec = importlib.util.spec_from_file_location("codex_usage_dashboard", MODULE_PATH)
assert spec is not None
dashboard = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(dashboard)

OPENER_PATH = (
    Path(__file__).resolve().parents[1]
    / "skills"
    / "codex-usage-dashboard"
    / "scripts"
    / "open_dashboard.py"
)

opener_spec = importlib.util.spec_from_file_location("open_dashboard", OPENER_PATH)
assert opener_spec is not None
opener = importlib.util.module_from_spec(opener_spec)
assert opener_spec.loader is not None
opener_spec.loader.exec_module(opener)


class CodexUsageDashboardTests(unittest.TestCase):
    def write_rollout_rows(
        self,
        codex_home: Path,
        thread_id: str,
        rows: list[dict],
    ) -> Path:
        sessions_dir = codex_home / "sessions"
        sessions_dir.mkdir(parents=True, exist_ok=True)
        path = sessions_dir / f"rollout-2026-07-10T10-00-00-{thread_id}.jsonl"
        path.write_text("\n".join(json.dumps(row) for row in rows), encoding="utf-8")
        return path

    @staticmethod
    def total_only_usage(total_tokens: int) -> dict:
        return {
            "input_tokens": total_tokens,
            "total_tokens": total_tokens,
        }

    def total_only_token_event(
        self,
        timestamp: str,
        cumulative_tokens: int,
        incremental_tokens: int,
    ) -> dict:
        return {
            "timestamp": timestamp,
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": self.total_only_usage(cumulative_tokens),
                    "last_token_usage": self.total_only_usage(incremental_tokens),
                },
            },
        }

    def write_parent_and_subagent_rollouts(self, codex_home: Path) -> tuple[Path, Path]:
        parent_id = "parent-thread"
        child_id = "child-thread"

        def usage(input_tokens: int, cached_tokens: int, output_tokens: int, total_tokens: int) -> dict:
            return {
                "input_tokens": input_tokens,
                "cached_input_tokens": cached_tokens,
                "output_tokens": output_tokens,
                "total_tokens": total_tokens,
            }

        def token_event(timestamp: str, cumulative: dict, incremental: dict) -> dict:
            return {
                "timestamp": timestamp,
                "type": "event_msg",
                "payload": {
                    "type": "token_count",
                    "info": {
                        "total_token_usage": cumulative,
                        "last_token_usage": incremental,
                    },
                },
            }

        parent_rows = [
            {
                "timestamp": "2026-07-10T10:00:00Z",
                "type": "session_meta",
                "payload": {
                    "id": parent_id,
                    "session_id": parent_id,
                    "thread_source": "user",
                    "cwd": "/work/shared-project",
                    "model": "gpt-5",
                },
            },
            token_event(
                "2026-07-10T10:01:00Z",
                usage(80, 20, 20, 100),
                usage(80, 20, 20, 100),
            ),
            token_event(
                "2026-07-10T10:05:00Z",
                usage(200, 50, 50, 250),
                usage(120, 30, 30, 150),
            ),
            token_event(
                "2026-07-10T10:20:00Z",
                usage(260, 60, 70, 330),
                usage(60, 10, 20, 80),
            ),
        ]
        child_rows = [
            {
                "timestamp": "2026-07-10T10:10:00.000Z",
                "type": "session_meta",
                "payload": {
                    "id": child_id,
                    "session_id": parent_id,
                    "thread_source": "subagent",
                    "source": {
                        "subagent": {
                            "thread_spawn": {
                                "parent_thread_id": parent_id,
                                "depth": 1,
                                "agent_path": "/root/audit",
                                "agent_nickname": "Audit",
                            }
                        }
                    },
                    "cwd": "/work/shared-project",
                    "model": "gpt-5",
                },
            },
            {
                "timestamp": "2026-07-10T10:10:00.001Z",
                "type": "session_meta",
                "payload": {
                    "id": parent_id,
                    "session_id": parent_id,
                    "thread_source": "user",
                    "cwd": "/work/shared-project",
                    "model": "gpt-5",
                },
            },
            token_event(
                "2026-07-10T10:10:00.002Z",
                usage(80, 20, 20, 100),
                usage(80, 20, 20, 100),
            ),
            token_event(
                "2026-07-10T10:10:00.003Z",
                usage(200, 50, 50, 250),
                usage(120, 30, 30, 150),
            ),
            {
                "timestamp": "2026-07-10T10:10:01Z",
                "type": "turn_context",
                "payload": {
                    "turn_id": "child-turn",
                    "cwd": "/work/shared-project",
                    "model": "gpt-5",
                },
            },
            token_event(
                "2026-07-10T10:11:00Z",
                usage(250, 60, 60, 310),
                usage(50, 10, 10, 60),
            ),
            token_event(
                "2026-07-10T10:12:00Z",
                usage(320, 80, 80, 400),
                usage(70, 20, 20, 90),
            ),
        ]

        return (
            self.write_rollout_rows(codex_home, parent_id, parent_rows),
            self.write_rollout_rows(codex_home, child_id, child_rows),
        )

    def write_usage_file(
        self,
        codex_home: Path,
        session_id: str,
        total_tokens: int,
        timestamp: str,
        cwd: str | None = None,
    ) -> Path:
        sessions_dir = codex_home / "sessions"
        sessions_dir.mkdir(parents=True, exist_ok=True)
        path = sessions_dir / f"rollout-2026-06-16T22-52-45-{session_id}.jsonl"
        rows = [
            {
                "timestamp": timestamp,
                "type": "session_meta",
                "payload": {
                    "id": session_id,
                    "cwd": cwd or f"/work/{session_id}",
                    "model": "gpt-5",
                },
            },
            {
                "timestamp": timestamp,
                "type": "event_msg",
                "payload": {
                    "type": "token_count",
                    "info": {
                        "total_token_usage": {"total_tokens": total_tokens},
                        "last_token_usage": {"total_tokens": total_tokens},
                    },
                },
            },
        ]
        path.write_text("\n".join(json.dumps(row) for row in rows), encoding="utf-8")
        return path

    def test_parse_file_skips_synthetic_context_for_title(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "rollout-2026-06-16T22-52-45-session.jsonl"
            rows = [
                {
                    "timestamp": "2026-06-16T14:52:45.653Z",
                    "type": "session_meta",
                    "payload": {
                        "id": "session",
                        "cwd": "/home/luyh7/game/immortal-advanture",
                        "model_provider": "custom",
                    },
                },
                {
                    "timestamp": "2026-06-16T14:52:45.677Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [
                            {
                                "type": "input_text",
                                "text": "# AGENTS.md instructions for /home/luyh7/game/immortal-advanture\n\n<INSTRUCTIONS>...",
                            },
                            {
                                "type": "input_text",
                                "text": "<environment_context>\n  <cwd>/home/luyh7/game/immortal-advanture</cwd>\n</environment_context>",
                            },
                        ],
                    },
                },
                {
                    "timestamp": "2026-06-16T14:52:45.728Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": [
                            {
                                "type": "input_text",
                                "text": "生成milestones0004的文档，如果现有文档有缺失的内容，询问我补充。\n",
                            }
                        ],
                    },
                },
                {
                    "timestamp": "2026-06-16T14:52:45.728Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "user_message",
                        "message": "生成milestones0004的文档，如果现有文档有缺失的内容，询问我补充。\n",
                    },
                },
            ]
            path.write_text("\n".join(json.dumps(row) for row in rows), encoding="utf-8")

            analyzer = dashboard.CodexUsageAnalyzer(Path(temp_dir))
            summary, detail = analyzer.parse_file(path, "active")

        expected = "生成milestones0004的文档，如果现有文档有缺失的内容，询问我补充。"
        self.assertEqual(summary["title"], expected)
        self.assertEqual(detail["first_user_prompt"], expected)

    def test_scan_skips_unused_large_payloads_without_changing_session_results(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            path = codex_home / "sessions" / "rollout-2026-07-10T10-00-00-fast-skip.jsonl"
            path.parent.mkdir(parents=True)
            rows = [
                {
                    "timestamp": "2026-07-10T09:00:00Z",
                    "type": "response_item",
                    "payload": {
                        "type": "function_call_output",
                        "output": "x" * 20_000 + '\\"type\\":\\"event_msg\\"',
                    },
                },
                {
                    "timestamp": "2026-07-10T10:00:00Z",
                    "type": "session_meta",
                    "payload": {
                        "id": "fast-skip",
                        "cwd": "/work/fast-skip",
                        "model": "gpt-5",
                    },
                },
                {
                    "timestamp": "2026-07-10T10:01:00Z",
                    "type": "response_item",
                    "payload": {
                        "type": "message",
                        "role": "user",
                        "content": "Optimize parser",
                    },
                },
                {
                    "timestamp": "2026-07-10T10:02:00Z",
                    "type": "response_item",
                    "payload": {"type": "function_call", "name": "shell"},
                },
                self.total_only_token_event("2026-07-10T10:03:00Z", 100, 100),
                {
                    "timestamp": "2026-07-10T10:30:00Z",
                    "type": "response_item",
                    "payload": {
                        "type": "image_generation_call",
                        "result": "a" * 20_000,
                    },
                },
                {
                    "timestamp": "2026-07-10T11:00:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "image_generation_end",
                        "result": "b" * 20_000,
                    },
                },
            ]
            malformed = (
                '{"timestamp":"2026-07-10T12:00:00Z","type":"response_item",'
                '"payload":{"type":"function_call_output","output":"truncated"}'
            )
            path.write_text(
                "\n".join(
                    [
                        *(json.dumps(row, separators=(",", ":")) for row in rows),
                        malformed,
                    ]
                ),
                encoding="utf-8",
            )

            snapshot = dashboard.CodexUsageAnalyzer(codex_home).scan()

        self.assertEqual(snapshot["summary"]["session_count"], 1)
        session = snapshot["sessions"][0]
        detail = snapshot["details_by_uid"][session["uid"]]
        self.assertEqual(session["title"], "Optimize parser")
        self.assertEqual(session["total_token_usage"]["total_tokens"], 100)
        self.assertEqual(detail["tool_counts"], {"shell": 1})
        self.assertEqual(detail["start_at"], "2026-07-10T09:00:00Z")
        self.assertEqual(detail["end_at"], "2026-07-10T11:00:00Z")
        self.assertEqual(detail["line_count"], 7)
        self.assertEqual(detail["parse_errors"], 0)
        self.assertEqual(detail["fast_skipped_line_count"], 3)
        self.assertGreater(detail["fast_skipped_bytes"], 60_000)

    def test_rollout_reader_streams_rows_and_tracks_consumed_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "rollout.jsonl"
            first = {"timestamp": "2026-07-10T10:00:00Z", "type": "world_state", "payload": {}}
            second = {"timestamp": "2026-07-10T10:01:00Z", "type": "world_state", "payload": {}}
            first_line = json.dumps(first, separators=(",", ":")) + "\n"
            second_line = json.dumps(second, separators=(",", ":")) + "\n"
            path.write_text(first_line + second_line, encoding="utf-8")
            stats = dashboard.RolloutReadStats()

            rows = dashboard.read_rollout_jsonl(path, stats=stats)
            self.assertEqual(stats.bytes_read, 0)
            self.assertEqual(next(rows), first)
            self.assertEqual(stats.bytes_read, len(first_line.encode("utf-8")))
            self.assertEqual(list(rows), [second])
            self.assertEqual(stats.bytes_read, path.stat().st_size)

    def test_subagent_identity_uses_first_session_meta(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            _parent_path, child_path = self.write_parent_and_subagent_rollouts(codex_home)
            analyzer = dashboard.CodexUsageAnalyzer(codex_home)

            summary, detail = analyzer.parse_file(child_path, "active")

        self.assertEqual(summary["session_id"], "child-thread")
        self.assertEqual(detail["session_id"], "child-thread")

    def test_subagent_usage_excludes_inherited_parent_timeline_prefix(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            self.write_parent_and_subagent_rollouts(codex_home)
            analyzer = dashboard.CodexUsageAnalyzer(codex_home)
            snapshot = analyzer.scan()

        child = next(
            row
            for row in snapshot["sessions"]
            if str(row["path"]).endswith("-child-thread.jsonl")
        )
        child_usage = child["total_token_usage"]
        self.assertEqual(child_usage["input_tokens"], 120)
        self.assertEqual(child_usage["cached_input_tokens"], 30)
        self.assertEqual(child_usage["output_tokens"], 30)
        self.assertEqual(child_usage["total_tokens"], 150)
        detail = snapshot["details_by_uid"][child["uid"]]
        self.assertEqual(detail["inherited_token_usage"]["total_tokens"], 250)
        self.assertEqual(
            [row["total_token_usage"]["total_tokens"] for row in detail["timeline"]],
            [60, 150],
        )
        ranged = analyzer.detail_for_period(
            detail,
            dt.datetime(2026, 7, 10, 10, 10, 30, tzinfo=dt.UTC),
            dt.datetime(2026, 7, 10, 10, 12, 30, tzinfo=dt.UTC),
        )
        self.assertEqual(ranged["total_token_usage"]["total_tokens"], 150)

    def test_parent_and_subagent_summary_does_not_double_count_inherited_usage(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            self.write_parent_and_subagent_rollouts(codex_home)
            snapshot = dashboard.CodexUsageAnalyzer(codex_home).scan()

        self.assertEqual(snapshot["summary"]["session_count"], 2)
        self.assertEqual(snapshot["summary"]["usage"]["total_tokens"], 480)
        self.assertEqual(len(snapshot["summary"]["by_project"]), 1)
        project = snapshot["summary"]["by_project"][0]
        self.assertEqual(project["sessions"], 2)
        self.assertEqual(project["usage"]["total_tokens"], 480)
        self.assertEqual(
            sum(row["usage"]["total_tokens"] for row in snapshot["daily_usage"]),
            480,
        )

    def test_subagent_before_parent_first_token_resolves_zero_baseline(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            parent_id = "parent-before-first-token"
            child_id = "child-before-first-token"
            self.write_rollout_rows(
                codex_home,
                parent_id,
                [
                    {
                        "timestamp": "2026-07-10T10:00:00Z",
                        "type": "session_meta",
                        "payload": {
                            "id": parent_id,
                            "session_id": parent_id,
                            "thread_source": "user",
                            "cwd": "/work/early-fork",
                            "model": "gpt-5",
                        },
                    },
                    self.total_only_token_event("2026-07-10T10:10:00Z", 100, 100),
                ],
            )
            self.write_rollout_rows(
                codex_home,
                child_id,
                [
                    {
                        "timestamp": "2026-07-10T10:05:00Z",
                        "type": "session_meta",
                        "payload": {
                            "id": child_id,
                            "session_id": parent_id,
                            "thread_source": "subagent",
                            "source": {
                                "subagent": {
                                    "thread_spawn": {
                                        "parent_thread_id": parent_id,
                                        "forked_from_id": parent_id,
                                        "agent_path": "/root/early-fork",
                                    }
                                }
                            },
                            "cwd": "/work/early-fork",
                            "model": "gpt-5",
                        },
                    },
                    self.total_only_token_event("2026-07-10T10:06:00Z", 60, 60),
                ],
            )

            snapshot = dashboard.CodexUsageAnalyzer(codex_home).scan()

        child = next(row for row in snapshot["sessions"] if row["session_id"] == child_id)
        detail = snapshot["details_by_uid"][child["uid"]]
        self.assertEqual(child["forked_from_id"], parent_id)
        self.assertTrue(child["fork_usage_resolved"])
        self.assertEqual(child["inherited_token_event_count"], 0)
        self.assertEqual(child["inherited_token_usage"]["total_tokens"], 0)
        self.assertEqual(child["total_token_usage"]["total_tokens"], 60)
        self.assertEqual(detail["branch_total_token_usage"]["total_tokens"], 60)
        self.assertEqual(
            [row["total_token_usage"]["total_tokens"] for row in detail["timeline"]],
            [60],
        )

    def test_subagent_last_n_slice_with_counter_reset_sums_event_deltas(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            parent_id = "parent-last-n"
            child_id = "child-last-n"
            parent_rows = [
                {
                    "timestamp": "2026-07-10T10:00:00Z",
                    "type": "session_meta",
                    "payload": {
                        "id": parent_id,
                        "session_id": parent_id,
                        "thread_source": "user",
                        "cwd": "/work/last-n",
                        "model": "gpt-5",
                    },
                },
                self.total_only_token_event("2026-07-10T10:01:00Z", 100, 100),
                self.total_only_token_event("2026-07-10T10:02:00Z", 200, 100),
                self.total_only_token_event("2026-07-10T10:03:00Z", 300, 100),
                self.total_only_token_event("2026-07-10T10:04:00Z", 400, 100),
                self.total_only_token_event("2026-07-10T10:05:00Z", 500, 100),
            ]
            child_rows = [
                {
                    "timestamp": "2026-07-10T10:06:00.000Z",
                    "type": "session_meta",
                    "payload": {
                        "id": child_id,
                        "session_id": parent_id,
                        "thread_source": "subagent",
                        "source": {
                            "subagent": {
                                "thread_spawn": {
                                    "parent_thread_id": parent_id,
                                    "forked_from_id": parent_id,
                                    "agent_path": "/root/last-n",
                                }
                            }
                        },
                        "cwd": "/work/last-n",
                        "model": "gpt-5",
                    },
                },
                self.total_only_token_event("2026-07-10T10:06:00.001Z", 200, 100),
                self.total_only_token_event("2026-07-10T10:06:00.002Z", 300, 100),
                self.total_only_token_event("2026-07-10T10:07:00Z", 360, 60),
                self.total_only_token_event("2026-07-10T10:08:00Z", 40, 40),
                self.total_only_token_event("2026-07-10T10:09:00Z", 90, 50),
            ]
            self.write_rollout_rows(codex_home, parent_id, parent_rows)
            self.write_rollout_rows(codex_home, child_id, child_rows)

            snapshot = dashboard.CodexUsageAnalyzer(codex_home).scan()

        child = next(row for row in snapshot["sessions"] if row["session_id"] == child_id)
        detail = snapshot["details_by_uid"][child["uid"]]
        self.assertTrue(child["fork_usage_resolved"])
        self.assertEqual(child["inherited_token_event_count"], 2)
        self.assertEqual(child["inherited_token_usage"]["total_tokens"], 300)
        self.assertEqual(child["branch_total_token_usage"]["total_tokens"], 90)
        self.assertEqual(child["total_token_usage"]["total_tokens"], 150)
        self.assertEqual(child["last_token_usage"]["total_tokens"], 50)
        self.assertEqual(
            [row["last_token_usage"]["total_tokens"] for row in detail["timeline"]],
            [60, 40, 50],
        )
        self.assertEqual(
            [row["total_token_usage"]["total_tokens"] for row in detail["timeline"]],
            [60, 100, 150],
        )

    def test_bounded_period_loads_old_fork_parent_dependency(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            parent_id = "parent-period-dependency"
            child_id = "child-period-dependency"
            parent_path = self.write_rollout_rows(
                codex_home,
                parent_id,
                [
                    {
                        "timestamp": "2026-07-08T13:00:00Z",
                        "type": "session_meta",
                        "payload": {
                            "id": parent_id,
                            "session_id": parent_id,
                            "thread_source": "user",
                            "cwd": "/work/period-dependency",
                            "model": "gpt-5",
                        },
                    },
                    self.total_only_token_event("2026-07-08T14:00:00Z", 100, 100),
                    self.total_only_token_event("2026-07-08T15:00:00Z", 250, 150),
                ],
            )
            child_path = self.write_rollout_rows(
                codex_home,
                child_id,
                [
                    {
                        "timestamp": "2026-07-09T01:00:00.000Z",
                        "type": "session_meta",
                        "payload": {
                            "id": child_id,
                            "session_id": parent_id,
                            "parent_thread_id": parent_id,
                            "forked_from_id": parent_id,
                            "thread_source": "subagent",
                            "cwd": "/work/period-dependency",
                            "model": "gpt-5",
                        },
                    },
                    self.total_only_token_event("2026-07-09T01:00:00.001Z", 100, 100),
                    self.total_only_token_event("2026-07-09T01:00:00.002Z", 250, 150),
                    self.total_only_token_event("2026-07-09T02:00:00Z", 320, 70),
                ],
            )
            _key, period_start, _period_end, _start_key, _end_key = dashboard.local_period_bounds(
                "custom",
                "2026-07-09",
                "2026-07-09",
            )
            assert period_start is not None
            parent_mtime = period_start.timestamp() - 1
            child_mtime = period_start.timestamp() + 1
            os.utime(parent_path, (parent_mtime, parent_mtime))
            os.utime(child_path, (child_mtime, child_mtime))

            analyzer = dashboard.CodexUsageAnalyzer(codex_home)
            parsed_paths: list[Path] = []
            original_parse_file = analyzer.parse_file

            def recording_parse_file(
                path: Path,
                source: str,
                log_source: dashboard.CodexLogSource | None = None,
                **kwargs,
            ) -> tuple[dict, dict]:
                parsed_paths.append(path.resolve())
                return original_parse_file(path, source, log_source, **kwargs)

            analyzer.parse_file = recording_parse_file
            snapshot = analyzer.scan("custom", "2026-07-09", "2026-07-09")

        self.assertIn(parent_path.resolve(), parsed_paths)
        self.assertIn(child_path.resolve(), parsed_paths)
        self.assertEqual([row["session_id"] for row in snapshot["sessions"]], [child_id])
        child = snapshot["sessions"][0]
        self.assertTrue(child["fork_usage_resolved"])
        self.assertEqual(child["inherited_token_event_count"], 2)
        self.assertEqual(child["inherited_token_usage"]["total_tokens"], 250)
        self.assertEqual(child["total_token_usage"]["total_tokens"], 70)
        self.assertEqual(snapshot["summary"]["usage"]["total_tokens"], 70)

    def test_missing_fork_parent_keeps_branch_total_but_excludes_exact_usage(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            missing_parent_id = "missing-parent"
            child_id = "child-with-missing-parent"
            self.write_usage_file(
                codex_home,
                "independent-thread",
                75,
                "2026-07-10T10:00:00Z",
                cwd="/work/missing-parent",
            )
            self.write_rollout_rows(
                codex_home,
                child_id,
                [
                    {
                        "timestamp": "2026-07-10T10:05:00Z",
                        "type": "session_meta",
                        "payload": {
                            "id": child_id,
                            "session_id": missing_parent_id,
                            "parent_thread_id": missing_parent_id,
                            "forked_from_id": missing_parent_id,
                            "thread_source": "subagent",
                            "cwd": "/work/missing-parent",
                            "model": "gpt-5",
                        },
                    },
                    self.total_only_token_event("2026-07-10T10:05:01Z", 250, 250),
                    self.total_only_token_event("2026-07-10T10:06:00Z", 400, 150),
                ],
            )

            snapshot = dashboard.CodexUsageAnalyzer(codex_home).scan()

        child = next(row for row in snapshot["sessions"] if row["session_id"] == child_id)
        detail = snapshot["details_by_uid"][child["uid"]]
        self.assertFalse(child["fork_usage_resolved"])
        self.assertEqual(child["branch_total_token_usage"]["total_tokens"], 400)
        self.assertEqual(child["total_token_usage"]["total_tokens"], 0)
        self.assertEqual(child["last_token_usage"]["total_tokens"], 0)
        self.assertEqual(child["token_event_count"], 0)
        self.assertEqual(detail["timeline"], [])
        self.assertEqual(snapshot["summary"]["session_count"], 2)
        self.assertEqual(snapshot["summary"]["usage"]["total_tokens"], 75)
        self.assertEqual(snapshot["summary"]["by_project"][0]["usage"]["total_tokens"], 75)

    def test_gpt_5_6_prices_start_at_beijing_launch_time(self) -> None:
        usage = {
            "input_tokens": 3_000_000,
            "cached_input_tokens": 1_000_000,
            "cache_write_tokens": 1_000_000,
            "output_tokens": 1_000_000,
            "total_tokens": 4_000_000,
        }

        self.assertIsNone(
            dashboard.estimate_cost_usd(
                usage,
                "gpt-5.6-sol",
                "2026-07-09T16:59:59.999999Z",
            )
        )
        self.assertEqual(
            dashboard.estimate_cost_usd(
                usage,
                "gpt-5.6-sol",
                "2026-07-09T17:00:00Z",
            ),
            68.5,
        )
        self.assertEqual(
            dashboard.estimate_cost_usd(
                usage,
                "gpt-5.6",
                "2026-07-10T01:00:00+08:00",
            ),
            68.5,
        )
        self.assertEqual(
            dashboard.estimate_cost_usd(
                usage,
                "gpt-5.6-terra",
                "2026-07-09T17:00:00Z",
            ),
            34.25,
        )
        self.assertEqual(
            dashboard.estimate_cost_usd(
                usage,
                "gpt-5.6-luna",
                "2026-07-09T17:00:00Z",
            ),
            13.7,
        )
        self.assertEqual(
            dashboard.price_for_model(
                "gpt-5.6-sol",
                "2026-07-09T17:00:00Z",
                input_tokens=272_000,
            ),
            dashboard.GPT_5_6_MODEL_PRICES_USD_PER_M_TOKENS["gpt-5.6-sol"],
        )
        self.assertEqual(
            dashboard.price_for_model(
                "gpt-5.6-sol",
                "2026-07-09T17:00:00Z",
                input_tokens=272_001,
            ),
            dashboard.GPT_5_6_LONG_CONTEXT_MODEL_PRICES_USD_PER_M_TOKENS["gpt-5.6-sol"],
        )

        legacy_usage = {
            "input_tokens": 1_000_000,
            "output_tokens": 1_000_000,
            "total_tokens": 2_000_000,
        }
        self.assertEqual(
            dashboard.estimate_cost_usd(legacy_usage, "gpt-5.4", "2026-07-09T16:59:59Z"),
            17.5,
        )
        self.assertEqual(
            dashboard.estimate_cost_usd(legacy_usage, "gpt-5.4", "2026-07-09T17:00:00Z"),
            17.5,
        )
        legacy_cache_write_usage = {
            "input_tokens": 1_000_000,
            "cache_write_tokens": 500_000,
            "total_tokens": 1_000_000,
        }
        self.assertEqual(
            dashboard.estimate_cost_usd(
                legacy_cache_write_usage,
                "gpt-5.4",
                "2026-07-09T16:59:59Z",
            ),
            2.5,
        )
        self.assertIsNone(dashboard.price_for_model("gpt-5.99"))

    def test_cross_launch_session_is_priced_by_event_model_and_time(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "rollout-2026-07-10T00-59-00-pricing.jsonl"
            rows = [
                {
                    "timestamp": "2026-07-09T16:59:00Z",
                    "type": "session_meta",
                    "payload": {
                        "id": "pricing-session",
                        "cwd": "/work/pricing",
                        "model": "gpt-5.4",
                    },
                },
                {
                    "timestamp": "2026-07-09T16:59:00Z",
                    "type": "turn_context",
                    "payload": {"turn_id": "before", "model": "gpt-5.4"},
                },
                {
                    "timestamp": "2026-07-09T16:59:59.999999Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "total_token_usage": {
                                "input_tokens": 1_000_000,
                                "cached_input_tokens": 0,
                                "cache_write_tokens": 0,
                                "output_tokens": 1_000_000,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 2_000_000,
                            },
                            "last_token_usage": {
                                "input_tokens": 1_000_000,
                                "cached_input_tokens": 0,
                                "cache_write_tokens": 0,
                                "output_tokens": 1_000_000,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 2_000_000,
                            },
                        },
                    },
                },
                {
                    "timestamp": "2026-07-09T17:00:00Z",
                    "type": "turn_context",
                    "payload": {"turn_id": "after", "model": "gpt-5.6-sol"},
                },
                {
                    "timestamp": "2026-07-09T17:00:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "total_token_usage": {
                                "input_tokens": 4_000_000,
                                "cached_input_tokens": 1_000_000,
                                "cache_write_tokens": 1_000_000,
                                "output_tokens": 2_000_000,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 6_000_000,
                            },
                            "last_token_usage": {
                                "input_tokens": 3_000_000,
                                "cached_input_tokens": 1_000_000,
                                "cache_write_tokens": 1_000_000,
                                "output_tokens": 1_000_000,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 4_000_000,
                            },
                        },
                    },
                },
            ]
            path.write_text("\n".join(json.dumps(row) for row in rows), encoding="utf-8")

            analyzer = dashboard.CodexUsageAnalyzer(Path(temp_dir))
            _summary, detail = analyzer.parse_file(path, "active")

            before = analyzer.detail_for_period(
                detail,
                dashboard.parse_timestamp("2026-07-09T16:00:00Z"),
                dashboard.parse_timestamp("2026-07-09T16:59:59.999999Z"),
            )
            after = analyzer.detail_for_period(
                detail,
                dashboard.parse_timestamp("2026-07-09T17:00:00Z"),
                dashboard.parse_timestamp("2026-07-09T18:00:00Z"),
            )

        self.assertEqual([row["model"] for row in detail["timeline"]], ["gpt-5.4", "gpt-5.6-sol"])
        self.assertEqual(detail["model"], "gpt-5.6-sol")
        self.assertEqual(detail["models"], ["gpt-5.4", "gpt-5.6-sol"])
        self.assertEqual(before["models"], ["gpt-5.4"])
        self.assertEqual(after["models"], ["gpt-5.6-sol"])
        self.assertEqual(detail["estimated_cost_usd"], 86.0)
        self.assertEqual(detail["estimated_cost_breakdown_usd"]["input_tokens"], 12.5)
        self.assertEqual(detail["estimated_cost_breakdown_usd"]["cached_input_tokens"], 1.0)
        self.assertEqual(detail["estimated_cost_breakdown_usd"]["cache_write_tokens"], 12.5)
        self.assertEqual(detail["estimated_cost_breakdown_usd"]["output_tokens"], 60.0)
        self.assertEqual([row["model"] for row in detail["applied_price_segments"]], ["gpt-5.4", "gpt-5.6-sol"])
        self.assertEqual([row["context_tier"] for row in detail["applied_price_segments"]], [None, "long"])
        self.assertEqual(before["estimated_cost_usd"], 17.5)
        self.assertEqual(after["estimated_cost_usd"], 68.5)
        self.assertEqual(detail["estimated_cost_usd"], before["estimated_cost_usd"] + after["estimated_cost_usd"])

    def test_thread_settings_applied_updates_model_without_turn_context(self) -> None:
        """Desktop/WSL often emits model switches via thread_settings_applied."""
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "rollout-2026-07-10T10-00-00-settings-model.jsonl"
            rows = [
                {
                    "timestamp": "2026-07-10T10:00:00Z",
                    "type": "session_meta",
                    "payload": {
                        "id": "settings-model-session",
                        "cwd": "/work/settings",
                        "model_provider": "custom",
                    },
                },
                {
                    "timestamp": "2026-07-10T10:00:01Z",
                    "type": "turn_context",
                    "payload": {
                        "turn_id": "t1",
                        "model": "gpt-5.6-sol",
                        "collaboration_mode": {
                            "mode": "default",
                            "settings": {"model": "gpt-5.6-sol"},
                        },
                    },
                },
                {
                    "timestamp": "2026-07-10T10:00:02Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "total_token_usage": {
                                "input_tokens": 100,
                                "cached_input_tokens": 0,
                                "output_tokens": 20,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 120,
                            },
                            "last_token_usage": {
                                "input_tokens": 100,
                                "cached_input_tokens": 0,
                                "output_tokens": 20,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 120,
                            },
                        },
                    },
                },
                {
                    "timestamp": "2026-07-10T10:01:00Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "thread_settings_applied",
                        "thread_settings": {
                            "model": "gpt-5.6-terra",
                            "reasoning_effort": "high",
                            "collaboration_mode": {
                                "mode": "default",
                                "settings": {"model": "gpt-5.6-terra"},
                            },
                        },
                    },
                },
                {
                    "timestamp": "2026-07-10T10:01:01Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "total_token_usage": {
                                "input_tokens": 300,
                                "cached_input_tokens": 50,
                                "output_tokens": 80,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 380,
                            },
                            "last_token_usage": {
                                "input_tokens": 200,
                                "cached_input_tokens": 50,
                                "output_tokens": 60,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 260,
                            },
                        },
                    },
                },
            ]
            path.write_text("\n".join(json.dumps(row) for row in rows), encoding="utf-8")

            analyzer = dashboard.CodexUsageAnalyzer(Path(temp_dir))
            summary, detail = analyzer.parse_file(path, "active")

        self.assertEqual(detail["model"], "gpt-5.6-terra")
        self.assertEqual(detail["models"], ["gpt-5.6-sol", "gpt-5.6-terra"])
        self.assertEqual(summary["models"], ["gpt-5.6-sol", "gpt-5.6-terra"])
        self.assertEqual(
            [row["model"] for row in detail["timeline"]],
            ["gpt-5.6-sol", "gpt-5.6-terra"],
        )
        self.assertEqual(detail["effort"], "high")
        self.assertEqual(
            dashboard.model_from_payload(
                {
                    "thread_settings": {
                        "collaboration_mode": {"settings": {"model": "gpt-5.6-terra"}}
                    }
                }
            ),
            "gpt-5.6-terra",
        )

    def test_session_models_tracks_all_switched_models(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "rollout-2026-07-10T10-00-00-multi-model.jsonl"
            rows = [
                {
                    "timestamp": "2026-07-10T10:00:00Z",
                    "type": "session_meta",
                    "payload": {
                        "id": "multi-model-session",
                        "cwd": "/work/multi",
                        "model": "gpt-5.4",
                    },
                },
                {
                    "timestamp": "2026-07-10T10:00:01Z",
                    "type": "turn_context",
                    "payload": {"turn_id": "t1", "model": "gpt-5.4"},
                },
                {
                    "timestamp": "2026-07-10T10:00:02Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "total_token_usage": {
                                "input_tokens": 100,
                                "cached_input_tokens": 0,
                                "output_tokens": 20,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 120,
                            },
                            "last_token_usage": {
                                "input_tokens": 100,
                                "cached_input_tokens": 0,
                                "output_tokens": 20,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 120,
                            },
                        },
                    },
                },
                {
                    "timestamp": "2026-07-10T10:01:00Z",
                    "type": "turn_context",
                    "payload": {"turn_id": "t2", "model": "gpt-5.6-terra"},
                },
                {
                    "timestamp": "2026-07-10T10:01:01Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "total_token_usage": {
                                "input_tokens": 300,
                                "cached_input_tokens": 50,
                                "output_tokens": 80,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 380,
                            },
                            "last_token_usage": {
                                "input_tokens": 200,
                                "cached_input_tokens": 50,
                                "output_tokens": 60,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 260,
                            },
                        },
                    },
                },
                {
                    "timestamp": "2026-07-10T10:02:00Z",
                    "type": "turn_context",
                    "payload": {"turn_id": "t3", "model": "gpt-5.4"},
                },
                {
                    "timestamp": "2026-07-10T10:02:01Z",
                    "type": "event_msg",
                    "payload": {
                        "type": "token_count",
                        "info": {
                            "total_token_usage": {
                                "input_tokens": 400,
                                "cached_input_tokens": 80,
                                "output_tokens": 100,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 500,
                            },
                            "last_token_usage": {
                                "input_tokens": 100,
                                "cached_input_tokens": 30,
                                "output_tokens": 20,
                                "reasoning_output_tokens": 0,
                                "total_tokens": 120,
                            },
                        },
                    },
                },
            ]
            path.write_text("\n".join(json.dumps(row) for row in rows), encoding="utf-8")

            analyzer = dashboard.CodexUsageAnalyzer(Path(temp_dir))
            summary, detail = analyzer.parse_file(path, "active")

        self.assertEqual(detail["model"], "gpt-5.4")
        self.assertEqual(detail["models"], ["gpt-5.4", "gpt-5.6-terra"])
        self.assertEqual(summary["models"], ["gpt-5.4", "gpt-5.6-terra"])
        self.assertEqual(
            [row["model"] for row in detail["timeline"]],
            ["gpt-5.4", "gpt-5.6-terra", "gpt-5.4"],
        )
        self.assertIn("models", dashboard.SUMMARY_KEYS)
        self.assertEqual(
            dashboard.unique_models(["gpt-5.4", "gpt-5.6-terra"], "gpt-5.4"),
            ["gpt-5.4", "gpt-5.6-terra"],
        )

    def test_period_usage_handles_cumulative_counter_resets(self) -> None:
        detail = {
            "uid": "reset-session",
            "model": "gpt-5",
            "timeline": [
                {
                    "timestamp": "2026-07-09T10:00:00Z",
                    "model": "gpt-5",
                    "total_token_usage": {"input_tokens": 100, "total_tokens": 100},
                },
                {
                    "timestamp": "2026-07-09T11:00:00Z",
                    "model": "gpt-5",
                    "total_token_usage": {"input_tokens": 20, "total_tokens": 20},
                },
                {
                    "timestamp": "2026-07-09T12:00:00Z",
                    "model": "gpt-5",
                    "total_token_usage": {"input_tokens": 50, "total_tokens": 50},
                },
            ],
            "tasks": [],
            "total_token_usage": {"input_tokens": 50, "total_tokens": 50},
            "start_at": "2026-07-09T10:00:00Z",
            "end_at": "2026-07-09T12:00:00Z",
        }
        analyzer = dashboard.CodexUsageAnalyzer(Path("/tmp/nonexistent-codex-home"))
        full = dashboard.pricing_for_timeline(detail["timeline"], "gpt-5")
        ranged = analyzer.detail_for_period(
            detail,
            dashboard.parse_timestamp("2026-07-09T11:00:00Z"),
            dashboard.parse_timestamp("2026-07-09T12:00:00Z"),
        )
        daily = dashboard.CodexUsageAnalyzer.build_daily_usage_static([detail])

        self.assertEqual(full["applied_price_segments"][0]["usage"]["input_tokens"], 150)
        self.assertEqual(ranged["total_token_usage"]["input_tokens"], 50)
        self.assertEqual(ranged["total_token_usage"]["total_tokens"], 50)
        self.assertEqual(daily[0]["usage"]["input_tokens"], 150)
        self.assertEqual(daily[0]["usage"]["total_tokens"], 150)

    def test_old_remote_snapshot_preserves_stored_cost_and_normalizes_cache_write_alias(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            store = dashboard.RemoteSnapshotStore("mac-local", Path(temp_dir) / "remotes")
            payload = {
                "schema": dashboard.SNAPSHOT_SCHEMA,
                "version": dashboard.SNAPSHOT_VERSION,
                "device": {"short_code": "mac-remote", "label": "Remote Mac"},
                "snapshot": {
                    "generated_at": "2026-07-09T17:01:00Z",
                    "sessions": [
                        {
                            "uid": "old-uid",
                            "session_id": "old-session",
                            "model": "gpt-5.6-sol",
                            "total_token_usage": {
                                "input_tokens": 200,
                                "cache_write_input_tokens": 25,
                                "total_tokens": 200,
                            },
                            "estimated_cost_usd": 17.5,
                            "estimated_cost_breakdown_usd": {"input_tokens": 17.5},
                            "price_model_known": True,
                        }
                    ],
                    "details_by_uid": {
                        "old-uid": {
                            "uid": "old-uid",
                            "session_id": "old-session",
                            "model": "gpt-5.6-sol",
                            "end_at": "2026-07-09T17:00:00Z",
                            "total_token_usage": {
                                "input_tokens": 200,
                                "cache_write_input_tokens": 25,
                                "total_tokens": 200,
                            },
                            "estimated_cost_usd": 17.5,
                            "estimated_cost_breakdown_usd": {"input_tokens": 17.5},
                            "price_model_known": True,
                            "timeline": [
                                {
                                    "timestamp": "2026-07-09T16:59:59Z",
                                    "total_token_usage": {"input_tokens": 100, "total_tokens": 100},
                                },
                                {
                                    "timestamp": "2026-07-09T17:00:00Z",
                                    "total_token_usage": {
                                        "input_tokens": 200,
                                        "cache_write_input_tokens": 25,
                                        "total_tokens": 200,
                                    },
                                },
                            ],
                        }
                    },
                },
            }
            self.assertTrue(store.import_snapshot(payload, label="Remote Mac")["ok"])
            sessions, details, _sources = store.transformed_sessions()

        self.assertEqual(sessions[0]["estimated_cost_usd"], 17.5)
        self.assertEqual(details[sessions[0]["uid"]]["estimated_cost_usd"], 17.5)
        self.assertEqual(sessions[0]["total_token_usage"]["cache_write_tokens"], 25)
        self.assertEqual(details[sessions[0]["uid"]]["timeline"][1]["total_token_usage"]["cache_write_tokens"], 25)

    def test_scan_combines_multiple_codex_homes(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            wsl_home = root / "wsl" / ".codex"
            windows_home = root / "windows" / ".codex"
            self.write_usage_file(wsl_home, "wsl-session", 100, "2026-06-16T14:52:45.653Z")
            self.write_usage_file(windows_home, "windows-session", 250, "2026-06-16T15:52:45.653Z")

            analyzer = dashboard.CodexUsageAnalyzer(
                [
                    dashboard.CodexLogSource("wsl", "WSL", wsl_home),
                    dashboard.CodexLogSource("windows", "Windows", windows_home),
                ]
            )
            snapshot = analyzer.scan()

        self.assertEqual(snapshot["summary"]["session_count"], 2)
        self.assertEqual(snapshot["summary"]["usage"]["total_tokens"], 350)
        self.assertEqual(len(snapshot["codex_sources"]), 2)

        by_environment = {row["id"]: row for row in snapshot["summary"]["by_environment"]}
        self.assertEqual(by_environment["wsl"]["sessions"], 1)
        self.assertEqual(by_environment["windows"]["sessions"], 1)
        self.assertEqual(by_environment["windows"]["usage"]["total_tokens"], 250)

        environments = {session["session_id"]: session["environment"] for session in snapshot["sessions"]}
        self.assertEqual(environments["wsl-session"], "WSL")
        self.assertEqual(environments["windows-session"], "Windows")

    def test_snapshot_token_keeps_list_and_detail_consistent(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            path = self.write_usage_file(
                codex_home,
                "snapshot-session",
                100,
                "2026-07-10T10:00:00Z",
            )
            analyzer = dashboard.CodexUsageAnalyzer(codex_home)

            first = analyzer.scan("all")
            first_token = first["snapshot_token"]
            first_uid = first["sessions"][0]["uid"]
            self.assertEqual(analyzer.scan("all")["snapshot_token"], first_token)

            self.write_usage_file(
                codex_home,
                "snapshot-session",
                250,
                "2026-07-10T10:00:00Z",
            )
            old_detail = analyzer.get_detail(
                first_uid,
                "all",
                snapshot_token=first_token,
            )
            self.assertIsNotNone(old_detail)
            assert old_detail is not None
            self.assertEqual(old_detail["total_token_usage"]["total_tokens"], 100)

            second = analyzer.scan("all")
            second_token = second["snapshot_token"]
            self.assertNotEqual(second_token, first_token)
            second_detail = analyzer.get_detail(
                first_uid,
                "all",
                snapshot_token=second_token,
            )
            self.assertIsNotNone(second_detail)
            assert second_detail is not None
            self.assertEqual(second_detail["total_token_usage"]["total_tokens"], 250)
            with self.assertRaises(dashboard.SnapshotStaleError):
                analyzer.get_detail(first_uid, "all", snapshot_token=first_token)

            self.assertEqual(path.stat().st_size, second["sessions"][0]["file_size"])

    def test_session_payload_cache_does_not_restore_an_unpublished_snapshot(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            self.write_usage_file(
                codex_home,
                "payload-race",
                100,
                "2026-07-10T10:00:00Z",
            )
            analyzer = dashboard.CodexUsageAnalyzer(codex_home)
            snapshot = analyzer.scan("all")
            token = snapshot["snapshot_token"]
            original_dumps = dashboard.json.dumps

            def unpublishing_dumps(*args, **kwargs):
                analyzer.unpublish_snapshot(snapshot)
                return original_dumps(*args, **kwargs)

            dashboard.json.dumps = unpublishing_dumps
            try:
                analyzer.session_payload_bytes(snapshot, {"snapshot_token": token})
            finally:
                dashboard.json.dumps = original_dumps

        self.assertNotIn(token, analyzer._published_snapshots)
        self.assertNotIn(token, analyzer._session_payload_cache)

    def test_persistent_parse_cache_reuses_unchanged_files_across_analyzers(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            codex_home = root / ".codex"
            cache_path = root / "cache" / "parsed-files.sqlite3"
            self.write_usage_file(
                codex_home,
                "persistent-session",
                100,
                "2026-07-10T10:00:00Z",
            )

            first_cache = dashboard.PersistentParseCache(cache_path)
            first_analyzer = dashboard.CodexUsageAnalyzer(
                codex_home,
                persistent_cache=first_cache,
            )
            first = first_analyzer.scan("all")
            first_cache.close()

            second_cache = dashboard.PersistentParseCache(cache_path)
            second_analyzer = dashboard.CodexUsageAnalyzer(
                codex_home,
                persistent_cache=second_cache,
            )
            second = second_analyzer.scan("all")
            second_cache.close()

        self.assertEqual(first["summary"], second["summary"])
        self.assertEqual(first_analyzer.cache_metrics["full_parses"], 1)
        self.assertEqual(second_analyzer.cache_metrics["full_parses"], 0)
        self.assertEqual(second_analyzer.cache_metrics["persistent_hits"], 1)

    def test_append_only_scan_parses_only_the_new_file_tail(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            path = codex_home / "sessions" / "rollout-2026-07-10T10-00-00-append.jsonl"
            path.parent.mkdir(parents=True)
            initial_rows = [
                {
                    "timestamp": "2026-07-10T10:00:00Z",
                    "type": "session_meta",
                    "payload": {
                        "id": "append-session",
                        "cwd": "/work/append-session",
                        "model": "gpt-5",
                    },
                },
                self.total_only_token_event("2026-07-10T10:01:00Z", 100, 100),
            ]
            path.write_text(
                "".join(
                    json.dumps(row, separators=(",", ":")) + "\n"
                    for row in initial_rows
                ),
                encoding="utf-8",
            )
            analyzer = dashboard.CodexUsageAnalyzer(codex_home)
            first = analyzer.scan("all")
            original_size = path.stat().st_size

            appended_rows = [
                {
                    "timestamp": "2026-07-10T10:01:30Z",
                    "type": "response_item",
                    "payload": {
                        "type": "function_call_output",
                        "output": "x" * 20_000,
                    },
                },
                self.total_only_token_event("2026-07-10T10:02:00Z", 250, 150),
            ]
            with path.open("a", encoding="utf-8") as handle:
                for row in appended_rows:
                    handle.write(json.dumps(row, separators=(",", ":")) + "\n")

            second = analyzer.scan("all")
            appended_bytes = path.stat().st_size - original_size
            second_detail = second["details_by_uid"][second["sessions"][0]["uid"]]

        self.assertEqual(first["summary"]["usage"]["total_tokens"], 100)
        self.assertEqual(second["summary"]["usage"]["total_tokens"], 250)
        self.assertEqual(analyzer.cache_metrics["full_parses"], 1)
        self.assertEqual(analyzer.cache_metrics["incremental_parses"], 1)
        self.assertEqual(
            analyzer.cache_metrics["incremental_bytes"],
            appended_bytes,
        )
        self.assertFalse(any(key.startswith("_") for key in second_detail))

    def test_append_resume_preserves_whether_session_metadata_was_seen(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            path = codex_home / "sessions" / "rollout-2026-07-10T10-00-00-late-meta.jsonl"
            path.parent.mkdir(parents=True)
            path.write_text(
                json.dumps(
                    self.total_only_token_event("2026-07-10T10:00:00Z", 100, 100),
                    separators=(",", ":"),
                )
                + "\n",
                encoding="utf-8",
            )
            analyzer = dashboard.CodexUsageAnalyzer(codex_home)
            first = analyzer.scan("all")

            appended_rows = [
                {
                    "timestamp": "2026-07-10T10:01:00Z",
                    "type": "session_meta",
                    "payload": {
                        "id": "real-session-id",
                        "cwd": "/work/late-meta",
                        "model": "gpt-5",
                    },
                },
                self.total_only_token_event("2026-07-10T10:02:00Z", 200, 100),
            ]
            with path.open("a", encoding="utf-8") as handle:
                for row in appended_rows:
                    handle.write(json.dumps(row, separators=(",", ":")) + "\n")
            second = analyzer.scan("all")

        self.assertNotEqual(first["sessions"][0]["session_id"], "real-session-id")
        self.assertEqual(second["sessions"][0]["session_id"], "real-session-id")
        self.assertEqual(analyzer.cache_metrics["incremental_parses"], 1)

    def test_incomplete_appended_line_resumes_from_last_complete_newline(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            path = codex_home / "sessions" / "rollout-2026-07-10T10-00-00-partial.jsonl"
            path.parent.mkdir(parents=True)
            initial_rows = [
                {
                    "timestamp": "2026-07-10T10:00:00Z",
                    "type": "session_meta",
                    "payload": {"id": "partial-session", "model": "gpt-5"},
                },
                self.total_only_token_event("2026-07-10T10:01:00Z", 100, 100),
            ]
            path.write_text(
                "".join(
                    json.dumps(row, separators=(",", ":")) + "\n"
                    for row in initial_rows
                ),
                encoding="utf-8",
            )
            analyzer = dashboard.CodexUsageAnalyzer(codex_home)
            analyzer.scan("all")

            completed_line = json.dumps(
                self.total_only_token_event("2026-07-10T10:02:00Z", 250, 150),
                separators=(",", ":"),
            )
            split_at = len(completed_line) // 2
            with path.open("a", encoding="utf-8") as handle:
                handle.write(completed_line[:split_at])
            partial = analyzer.scan("all")
            partial_detail = partial["details_by_uid"][partial["sessions"][0]["uid"]]

            with path.open("a", encoding="utf-8") as handle:
                handle.write(completed_line[split_at:] + "\n")
            completed = analyzer.scan("all")
            completed_detail = completed["details_by_uid"][completed["sessions"][0]["uid"]]

        self.assertEqual(partial_detail["parse_errors"], 0)
        self.assertEqual(completed_detail["parse_errors"], 0)
        self.assertEqual(completed["summary"]["usage"]["total_tokens"], 250)
        self.assertEqual(analyzer.cache_metrics["incremental_parses"], 2)
        self.assertEqual(analyzer.cache_metrics["full_parses"], 1)

    def test_same_size_rewrite_during_serial_parse_is_retried(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            path = self.write_usage_file(
                codex_home,
                "serial-rewrite",
                100,
                "2026-07-10T10:00:00Z",
            )

            class RewritingAnalyzer(dashboard.CodexUsageAnalyzer):
                rewrote = False

                def parse_file(self, *args, **kwargs):
                    parsed = super().parse_file(*args, **kwargs)
                    if not self.rewrote:
                        before = path.stat()
                        path.write_text(
                            path.read_text(encoding="utf-8").replace("100", "200"),
                            encoding="utf-8",
                        )
                        os.utime(
                            path,
                            ns=(before.st_atime_ns, before.st_mtime_ns + 1_000_000_000),
                        )
                        self.rewrote = True
                    return parsed

            analyzer = RewritingAnalyzer(codex_home)
            snapshot = analyzer.scan("all")

        self.assertEqual(snapshot["summary"]["usage"]["total_tokens"], 200)
        self.assertEqual(analyzer.cache_metrics["full_parses"], 1)

    def test_append_during_parse_is_deferred_to_next_incremental_scan(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            path = self.write_usage_file(
                codex_home,
                "append-during-parse",
                100,
                "2026-07-10T10:00:00Z",
            )
            with path.open("a", encoding="utf-8") as handle:
                handle.write("\n")

            class AppendingAnalyzer(dashboard.CodexUsageAnalyzer):
                parse_calls = 0

                def parse_file(self, *args, **kwargs):
                    self.parse_calls += 1
                    parsed = super().parse_file(*args, **kwargs)
                    if self.parse_calls == 1:
                        with path.open("a", encoding="utf-8") as handle:
                            handle.write(
                                json.dumps(
                                    self_test.total_only_token_event(
                                        "2026-07-10T10:01:00Z",
                                        250,
                                        150,
                                    ),
                                    separators=(",", ":"),
                                )
                                + "\n"
                            )
                    return parsed

            self_test = self
            analyzer = AppendingAnalyzer(codex_home)
            first = analyzer.scan("all")
            second = analyzer.scan("all")

        self.assertEqual(first["summary"]["usage"]["total_tokens"], 100)
        self.assertEqual(second["summary"]["usage"]["total_tokens"], 250)
        self.assertEqual(analyzer.parse_calls, 2)
        self.assertEqual(analyzer.cache_metrics["full_parses"], 1)
        self.assertEqual(analyzer.cache_metrics["incremental_parses"], 1)

    def test_parallel_parse_discards_result_when_file_changes_after_worker_stat(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            path = self.write_usage_file(
                codex_home,
                "parallel-rewrite",
                100,
                "2026-07-10T10:00:00Z",
            )
            source = dashboard.make_log_source(codex_home)
            analyzer = dashboard.CodexUsageAnalyzer(
                [source],
                parallel_workers=2,
            )

            class MutatingPool:
                def map(self, function, jobs, chunksize=1):
                    results = [function(job) for job in jobs]
                    before = path.stat()
                    path.write_text(
                        path.read_text(encoding="utf-8").replace("100", "200"),
                        encoding="utf-8",
                    )
                    os.utime(
                        path,
                        ns=(before.st_atime_ns, before.st_mtime_ns + 1_000_000_000),
                    )
                    return results

            analyzer.process_pool = lambda: MutatingPool()
            parsed = analyzer.parse_files_in_parallel(
                [(0, (source, path, "active"))]
            )

        self.assertEqual(parsed, {})

    def test_parallel_parse_falls_back_only_for_a_missing_file(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            valid_path = self.write_usage_file(
                codex_home,
                "parallel-valid",
                100,
                "2026-07-10T10:00:00Z",
            )
            missing_path = valid_path.with_name("rollout-missing.jsonl")
            source = dashboard.make_log_source(codex_home)
            analyzer = dashboard.CodexUsageAnalyzer([source], parallel_workers=2)

            class InlinePool:
                def map(self, function, jobs, chunksize=1):
                    return [function(job) for job in jobs]

            analyzer.process_pool = lambda: InlinePool()
            parsed = analyzer.parse_files_in_parallel(
                [
                    (0, (source, valid_path, "active")),
                    (1, (source, missing_path, "active")),
                ]
            )

        self.assertIsNotNone(parsed)
        assert parsed is not None
        self.assertIn(0, parsed)
        self.assertNotIn(1, parsed)
        self.assertFalse(analyzer._parallel_disabled)

    def test_replaced_file_identity_does_not_use_append_resume(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            path = self.write_usage_file(
                codex_home,
                "replaced-session",
                100,
                "2026-07-10T10:00:00Z",
            )
            with path.open("a", encoding="utf-8") as handle:
                handle.write("\n")
            analyzer = dashboard.CodexUsageAnalyzer(codex_home)
            analyzer.scan("all")

            replacement = path.with_suffix(".replacement")
            replacement_rows = [
                {
                    "timestamp": "2026-07-10T10:00:00Z",
                    "type": "session_meta",
                    "payload": {"id": "replaced-session", "model": "gpt-5"},
                },
                self.total_only_token_event("2026-07-10T10:01:00Z", 250, 250),
            ]
            replacement.write_text(
                "".join(
                    json.dumps(row, separators=(",", ":")) + "\n"
                    for row in replacement_rows
                ),
                encoding="utf-8",
            )
            os.replace(replacement, path)
            second = analyzer.scan("all")

        self.assertEqual(second["summary"]["usage"]["total_tokens"], 250)
        self.assertEqual(analyzer.cache_metrics["incremental_parses"], 0)
        self.assertEqual(analyzer.cache_metrics["full_parses"], 2)

    def test_truncated_file_does_not_use_append_resume(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            path = self.write_usage_file(
                codex_home,
                "truncated-session",
                100,
                "2026-07-10T10:00:00Z",
            )
            analyzer = dashboard.CodexUsageAnalyzer(codex_home)
            analyzer.scan("all")

            rows = [
                {
                    "timestamp": "2026-07-10T10:00:00Z",
                    "type": "session_meta",
                    "payload": {"id": "truncated-session", "model": "gpt-5"},
                },
                self.total_only_token_event("2026-07-10T10:01:00Z", 50, 50),
            ]
            path.write_text(
                "".join(json.dumps(row, separators=(",", ":")) + "\n" for row in rows),
                encoding="utf-8",
            )
            snapshot = analyzer.scan("all")

        self.assertEqual(snapshot["summary"]["usage"]["total_tokens"], 50)
        self.assertEqual(analyzer.cache_metrics["incremental_parses"], 0)
        self.assertEqual(analyzer.cache_metrics["full_parses"], 2)

    def test_old_parser_version_invalidates_persistent_cache(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            codex_home = root / ".codex"
            cache_path = root / "cache" / "parsed-files.sqlite3"
            self.write_usage_file(
                codex_home,
                "versioned-session",
                100,
                "2026-07-10T10:00:00Z",
            )
            first = dashboard.CodexUsageAnalyzer(
                codex_home,
                persistent_cache=dashboard.PersistentParseCache(cache_path),
            )
            first.scan("all")
            first.close()
            with sqlite3.connect(cache_path) as connection:
                connection.execute(
                    "UPDATE parsed_files SET parser_version = ?",
                    (dashboard.PARSE_CACHE_VERSION - 1,),
                )
                connection.commit()

            second = dashboard.CodexUsageAnalyzer(
                codex_home,
                persistent_cache=dashboard.PersistentParseCache(cache_path),
            )
            snapshot = second.scan("all")
            second.close()

        self.assertEqual(snapshot["summary"]["session_count"], 1)
        self.assertEqual(second.cache_metrics["persistent_hits"], 0)
        self.assertEqual(second.cache_metrics["full_parses"], 1)

    def test_persistent_cache_prunes_files_missing_from_full_inventory(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            codex_home = root / ".codex"
            cache_path = root / "cache" / "parsed-files.sqlite3"
            path = self.write_usage_file(
                codex_home,
                "deleted-session",
                100,
                "2026-07-10T10:00:00Z",
            )
            first = dashboard.CodexUsageAnalyzer(
                codex_home,
                persistent_cache=dashboard.PersistentParseCache(cache_path),
            )
            first.scan("all")
            first.close()
            path.unlink()

            second = dashboard.CodexUsageAnalyzer(
                codex_home,
                persistent_cache=dashboard.PersistentParseCache(cache_path),
            )
            second.scan("all")
            second.close()
            with sqlite3.connect(cache_path) as connection:
                row_count = connection.execute("SELECT COUNT(*) FROM parsed_files").fetchone()[0]

        self.assertEqual(row_count, 0)

    def test_persistent_cache_prune_preserves_other_codex_home_scopes(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            home_a = root / "a" / ".codex"
            home_b = root / "b" / ".codex"
            cache_path = root / "cache" / "parsed-files.sqlite3"
            self.write_usage_file(home_a, "home-a", 100, "2026-07-10T10:00:00Z")
            self.write_usage_file(home_b, "home-b", 200, "2026-07-10T10:00:00Z")

            first_a = dashboard.CodexUsageAnalyzer(
                home_a,
                persistent_cache=dashboard.PersistentParseCache(cache_path),
            )
            first_a.scan("all")
            first_a.close()
            first_b = dashboard.CodexUsageAnalyzer(
                home_b,
                persistent_cache=dashboard.PersistentParseCache(cache_path),
            )
            first_b.scan("all")
            first_b.close()
            second_a = dashboard.CodexUsageAnalyzer(
                home_a,
                persistent_cache=dashboard.PersistentParseCache(cache_path),
            )
            second_a.scan("all")
            second_a.close()

        self.assertEqual(second_a.cache_metrics["persistent_hits"], 1)
        self.assertEqual(second_a.cache_metrics["full_parses"], 0)

    def test_persistent_cache_rolls_back_failed_batch(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            cache = dashboard.PersistentParseCache(Path(temp_dir) / "parsed-files.sqlite3")
            connection = cache.connection()
            assert connection is not None
            connection.execute(
                """
                CREATE TRIGGER reject_parsed_file
                BEFORE INSERT ON parsed_files
                BEGIN
                    SELECT RAISE(ABORT, 'injected failure');
                END
                """
            )
            connection.commit()
            entry = dashboard.FileParseCacheEntry(
                1,
                1,
                1,
                1,
                {"uid": "summary"},
                {"uid": "detail"},
                True,
                "prefix",
                "tail",
            )

            cache.put_many([("cache-key", entry)])

            self.assertFalse(connection.in_transaction)
            cache.close()

    def test_scan_lock_serializes_concurrent_snapshot_builds(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            self.write_usage_file(
                codex_home,
                "concurrent-session",
                100,
                "2026-07-10T10:00:00Z",
            )

            class CountingAnalyzer(dashboard.CodexUsageAnalyzer):
                def __init__(self, *args, **kwargs):
                    super().__init__(*args, **kwargs)
                    self.build_count = 0
                    self.count_lock = threading.Lock()

                def build_snapshot(self, *args, **kwargs):
                    with self.count_lock:
                        self.build_count += 1
                    time.sleep(0.03)
                    return super().build_snapshot(*args, **kwargs)

            analyzer = CountingAnalyzer(codex_home)
            ready = threading.Barrier(5)
            snapshots = []

            def scan() -> None:
                ready.wait()
                snapshots.append(analyzer.scan("all"))

            threads = [threading.Thread(target=scan) for _ in range(4)]
            for thread in threads:
                thread.start()
            ready.wait()
            for thread in threads:
                thread.join(timeout=2)

        self.assertTrue(all(not thread.is_alive() for thread in threads))
        self.assertEqual(analyzer.build_count, 1)
        self.assertEqual(len({snapshot["snapshot_token"] for snapshot in snapshots}), 1)

    def test_append_resume_does_not_treat_mtime_fallback_as_event_time(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            path = codex_home / "sessions" / "rollout-2026-07-10T10-00-00-time-fallback.jsonl"
            path.parent.mkdir(parents=True)
            path.write_text('{"type":"world_state","payload":{}}\n', encoding="utf-8")
            fallback_time = dt.datetime(2026, 7, 10, 12, 0, tzinfo=dt.UTC).timestamp()
            os.utime(path, (fallback_time, fallback_time))
            analyzer = dashboard.CodexUsageAnalyzer(codex_home)
            first = analyzer.scan("all")

            with path.open("a", encoding="utf-8") as handle:
                handle.write(
                    json.dumps(
                        self.total_only_token_event("2026-07-10T09:00:00Z", 100, 100),
                        separators=(",", ":"),
                    )
                    + "\n"
                )
            second = analyzer.scan("all")

        self.assertNotEqual(first["sessions"][0]["start_at"], "2026-07-10T09:00:00Z")
        self.assertEqual(second["sessions"][0]["start_at"], "2026-07-10T09:00:00Z")
        self.assertEqual(second["sessions"][0]["end_at"], "2026-07-10T09:00:00Z")

    def test_bounded_period_skips_logs_not_modified_since_period_start(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            _key, period_start, _period_end, _start_key, _end_key = dashboard.local_period_bounds("today")
            assert period_start is not None
            old_timestamp = dashboard.utc_iso(period_start - dt.timedelta(seconds=1))
            current_timestamp = dashboard.utc_iso(dt.datetime.now(dt.UTC))
            old_path = self.write_usage_file(codex_home, "old-session", 100, old_timestamp)
            current_path = self.write_usage_file(codex_home, "current-session", 250, current_timestamp)
            old_mtime = period_start.timestamp() - 1
            os.utime(old_path, (old_mtime, old_mtime))

            analyzer = dashboard.CodexUsageAnalyzer(codex_home)
            parsed_paths: list[Path] = []
            original_parse_file = analyzer.parse_file

            def recording_parse_file(
                path: Path,
                source: str,
                log_source: dashboard.CodexLogSource | None = None,
                **kwargs,
            ) -> tuple[dict, dict]:
                parsed_paths.append(path)
                return original_parse_file(path, source, log_source, **kwargs)

            analyzer.parse_file = recording_parse_file
            today = analyzer.scan("today")

            self.assertEqual([row["session_id"] for row in today["sessions"]], ["current-session"])
            self.assertIn(current_path, parsed_paths)
            self.assertNotIn(old_path, parsed_paths)
            self.assertFalse(today["daily_usage_complete"])

            parsed_paths.clear()
            all_time = analyzer.scan("all")

            self.assertEqual({row["session_id"] for row in all_time["sessions"]}, {"old-session", "current-session"})
            self.assertIn(old_path, parsed_paths)
            self.assertTrue(all_time["daily_usage_complete"])

    def test_windows_cwd_uses_folder_name_for_project(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            self.write_usage_file(
                codex_home,
                "windows-session",
                100,
                "2026-06-16T14:52:45.653Z",
                cwd=r"C:\Users\luyh7\game\codex-usage-dashboard",
            )

            analyzer = dashboard.CodexUsageAnalyzer(
                [dashboard.CodexLogSource("windows", "Windows", codex_home)]
            )
            snapshot = analyzer.scan()

        session = snapshot["sessions"][0]
        self.assertEqual(session["project"], "codex-usage-dashboard")
        self.assertEqual(session["title"], "codex-usage-dashboard")

    def test_git_worktree_sessions_group_under_main_project_root(self) -> None:
        if shutil.which("git") is None:
            self.skipTest("git is not available")

        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            repo = root / "codex-usage-dashboard"
            worktree = root / "codex-usage-dashboard-feature"
            subprocess.run(["git", "init", "-b", "main", str(repo)], check=True, capture_output=True, text=True)
            (repo / "README.md").write_text("test\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(repo), "add", "README.md"], check=True, capture_output=True, text=True)
            subprocess.run(
                ["git", "-C", str(repo), "-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-m", "init"],
                check=True,
                capture_output=True,
                text=True,
            )
            subprocess.run(
                ["git", "-C", str(repo), "worktree", "add", "-b", "feature/worktree-stats", str(worktree)],
                check=True,
                capture_output=True,
                text=True,
            )

            codex_home = root / ".codex"
            self.write_usage_file(codex_home, "main-session", 100, "2026-06-16T14:52:45.653Z", cwd=str(repo))
            self.write_usage_file(codex_home, "worktree-session", 250, "2026-06-16T15:52:45.653Z", cwd=str(worktree))

            analyzer = dashboard.CodexUsageAnalyzer([dashboard.CodexLogSource("local", "Local", codex_home)])
            snapshot = analyzer.scan()

        by_session = {row["session_id"]: row for row in snapshot["sessions"]}
        self.assertEqual(by_session["main-session"]["project"], "codex-usage-dashboard")
        self.assertEqual(by_session["worktree-session"]["project"], "codex-usage-dashboard")
        self.assertEqual(by_session["main-session"]["project_root"], str(repo.resolve()))
        self.assertEqual(by_session["worktree-session"]["project_root"], str(repo.resolve()))
        self.assertEqual(by_session["worktree-session"]["workspace_root"], str(worktree.resolve()))
        self.assertEqual(by_session["worktree-session"]["project_branch"], "feature/worktree-stats")
        self.assertTrue(by_session["worktree-session"]["is_git_worktree"])

        by_project = {row["project_root"]: row for row in snapshot["summary"]["by_project"]}
        self.assertEqual(by_project[str(repo.resolve())]["sessions"], 2)
        self.assertEqual(by_project[str(repo.resolve())]["usage"]["total_tokens"], 350)

    def test_snapshot_export_contains_device_short_code(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            codex_home = root / ".codex"
            self.write_usage_file(codex_home, "session-a", 100, "2026-06-16T14:52:45.653Z")
            store = dashboard.RemoteSnapshotStore("mac-test123", root / "remotes")
            analyzer = dashboard.CodexUsageAnalyzer([dashboard.CodexLogSource("local", "Local", codex_home)], remote_store=store)
            payload = analyzer.export_snapshot_payload()

        self.assertEqual(payload["schema"], dashboard.SNAPSHOT_SCHEMA)
        self.assertEqual(payload["device"]["short_code"], "mac-test123")
        self.assertEqual(payload["snapshot"]["summary"]["session_count"], 1)

    def test_remote_import_merges_by_device_short_code(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            store = dashboard.RemoteSnapshotStore("mac-local", root / "remotes")
            first = {
                "schema": dashboard.SNAPSHOT_SCHEMA,
                "version": dashboard.SNAPSHOT_VERSION,
                "device": {"short_code": "mac-remote", "label": "Mac Studio"},
                "snapshot": {
                    "generated_at": "2026-06-16T14:52:45Z",
                    "sessions": [
                        {
                            "uid": "old-uid",
                            "session_id": "same-session",
                            "title": "Old",
                            "source": "active",
                            "environment": "macOS",
                            "environment_id": "macos",
                            "is_remote": False,
                            "remote_device_short_code": "",
                            "remote_imported_at": "",
                            "remote_exported_at": "",
                            "codex_home": "/remote/.codex",
                            "path": "/remote/old.jsonl",
                            "file_size": 1,
                            "parse_errors": 0,
                            "created_at": "2026-06-16T14:52:45Z",
                            "start_at": "2026-06-16T14:52:45Z",
                            "end_at": "2026-06-16T14:52:45Z",
                            "updated_at": "2026-06-16T14:52:45Z",
                            "cwd": "/work/old",
                            "project": "old",
                            "model": "gpt-5",
                            "effort": "",
                            "total_token_usage": {"total_tokens": 100},
                            "last_token_usage": {"total_tokens": 100},
                            "estimated_cost_usd": None,
                            "estimated_cost_breakdown_usd": None,
                            "price_model_known": False,
                            "cached_input_percent": None,
                            "token_event_count": 1,
                            "turn_count": 1,
                            "completed_turn_count": 0,
                            "duration_ms_total": 0,
                            "time_to_first_token_ms_avg": None,
                        }
                    ],
                    "details_by_uid": {"old-uid": {"uid": "old-uid", "session_id": "same-session", "timeline": []}},
                },
            }
            second = json.loads(json.dumps(first))
            second["snapshot"]["sessions"][0]["uid"] = "new-uid"
            second["snapshot"]["sessions"][0]["title"] = "New"
            second["snapshot"]["sessions"][0]["total_token_usage"] = {"total_tokens": 250}
            second["snapshot"]["details_by_uid"] = {"new-uid": {"uid": "new-uid", "session_id": "same-session", "timeline": []}}

            self.assertTrue(store.import_snapshot(first, label="Mac Studio")["ok"])
            self.assertTrue(store.import_snapshot(second)["ok"])
            payload = store.read_remote("mac-remote")

        assert payload is not None
        sessions = payload["snapshot"]["sessions"]
        self.assertEqual(len(sessions), 1)
        self.assertEqual(sessions[0]["title"], "New")
        self.assertEqual(sessions[0]["total_token_usage"]["total_tokens"], 250)

    def test_remote_scan_marks_imported_sessions(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            local_home = root / "local" / ".codex"
            remote_home = root / "remote" / ".codex"
            self.write_usage_file(local_home, "local-session", 100, "2026-06-16T14:52:45.653Z")
            self.write_usage_file(remote_home, "remote-session", 250, "2026-06-16T15:52:45.653Z")

            remote_analyzer = dashboard.CodexUsageAnalyzer(
                [dashboard.CodexLogSource("remote-src", "Remote Source", remote_home)],
                remote_store=dashboard.RemoteSnapshotStore("mac-remote", root / "unused"),
            )
            remote_payload = remote_analyzer.export_snapshot_payload()
            store = dashboard.RemoteSnapshotStore("mac-local", root / "remotes")
            store.import_snapshot(remote_payload, label="Mac Studio")

            analyzer = dashboard.CodexUsageAnalyzer(
                [dashboard.CodexLogSource("local", "This Mac", local_home)],
                remote_store=store,
            )
            snapshot = analyzer.scan()

        by_session = {row["session_id"]: row for row in snapshot["sessions"]}
        self.assertFalse(by_session["local-session"]["is_remote"])
        self.assertTrue(by_session["remote-session"]["is_remote"])
        self.assertEqual(by_session["remote-session"]["environment"], "Mac Studio")
        self.assertEqual(by_session["remote-session"]["remote_device_short_code"], "mac-remote")

    def test_current_device_import_requires_confirmation(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            store = dashboard.RemoteSnapshotStore("mac-local", Path(temp_dir) / "remotes")
            payload = {
                "schema": dashboard.SNAPSHOT_SCHEMA,
                "version": dashboard.SNAPSHOT_VERSION,
                "device": {"short_code": "mac-local", "label": "This Mac"},
                "snapshot": {"sessions": [], "details_by_uid": {}},
            }
            result = store.import_snapshot(payload, label="This Mac")

        self.assertFalse(result["ok"])
        self.assertTrue(result["needs_confirmation"])
        self.assertEqual(result["reason"], "current_device")

    def test_html_defaults_to_project_grouped_view(self) -> None:
        html = dashboard.HTML
        project_index = html.index('data-view-button="project"')
        recent_index = html.index('data-view-button="recent"')
        total_index = html.index('data-view-button="total"')

        self.assertLess(project_index, recent_index)
        self.assertLess(recent_index, total_index)
        self.assertIn("viewMode: 'project'", html)
        self.assertIn("const projectPreviewLimit = 5", html)
        self.assertIn("function renderProjectTable()", html)
        self.assertIn("function projectGroups()", html)
        self.assertIn("function taskGroupsForRows(rows)", html)
        self.assertIn("row.root_session_id", html)
        self.assertIn("row.parent_thread_id", html)
        self.assertIn("usage = addClientUsage(usage, usageOf(row));", html)
        self.assertIn('data-task-toggle="${escapeHtml(task.key)}"', html)
        self.assertIn("taskExpanded: {}", html)
        self.assertIn("task-detail-summary", html)
        self.assertIn("selectable: group.subagentCount === 0", html)
        self.assertIn("sessionRowHtml(group.root, childClasses, options)", html)
        self.assertIn("data-task-toggle-row", html)
        self.assertIn("function toggleTaskGroup(key)", html)
        self.assertIn(".project-session-row .title-cell {\n      padding-left: 34px;", html)
        self.assertIn(".project-session-row.task-expandable-row .title-cell {\n      padding-left: 34px;", html)
        self.assertIn(".project-session-row.task-child-row .title-cell {\n      padding-left: 74px;", html)
        self.assertNotIn("mainAgent: '主 agent'", html)
        self.assertIn(".project-session-row.task-child-row .title-cell", html)
        self.assertNotIn("task-indent", html)
        self.assertNotIn("taskAgentCount", html)
        self.assertIn("function folderIcon()", html)
        self.assertIn("function branchIcon()", html)
        self.assertIn("function branchBadge(row)", html)
        self.assertIn("return row.project_root || row.cwd", html)
        self.assertIn("${branchIcon()}</span>", html)
        self.assertNotIn("${branchIcon()}${escapeHtml(label)}</span>", html)
        self.assertIn("compactProject", html)
        self.assertIn("archivedDelta", html)
        self.assertIn("function summarizeProjectRows(rows)", html)
        self.assertIn('class="number project-aggregate-cell project-total-cell"', html)
        self.assertIn('class="model-cell project-aggregate-cell"', html)
        self.assertNotIn('<td class="project-summary-cell" colspan="6">', html)
        self.assertIn("--project-summary: #2d5fa8;", html)
        self.assertLess(html.index("${folderIcon()}"), html.index('<span class="project-name"'))
        self.assertLess(html.index('<div class="project-title-cell">'), html.index("${environmentBadge(group)}"))
        self.assertLess(html.index("${environmentBadge(group)}"), html.index('<div class="project-meta">'))
        self.assertIn("git-worktree-project-grouping", dashboard.DASHBOARD_FEATURES)
        self.assertIn("effective-dated-pricing-v1", dashboard.DASHBOARD_FEATURES)
        self.assertIn("expandable-agent-task-rollups-v1", dashboard.DASHBOARD_FEATURES)
        self.assertIn("compact-agent-task-tree-v1", dashboard.DASHBOARD_FEATURES)
        self.assertIn("aligned-agent-task-tree-v1", dashboard.DASHBOARD_FEATURES)
        self.assertIn("main-agent-child-usage-row-v1", dashboard.DASHBOARD_FEATURES)
        self.assertIn("aligned-task-title-gutter-v1", dashboard.DASHBOARD_FEATURES)
        self.assertIn("project-session-indent-v1", dashboard.DASHBOARD_FEATURES)
        self.assertIn("clickable-task-rows-and-gutter-v1", dashboard.DASHBOARD_FEATURES)
        self.assertIn("main-agent-title-alignment-v1", dashboard.DASHBOARD_FEATURES)
        self.assertIn("compact-subagent-indent-v1", dashboard.DASHBOARD_FEATURES)
        self.assertIn("project-folder-aggregate-columns-v1", dashboard.DASHBOARD_FEATURES)
        self.assertIn("function unitPriceTooltip(segments, usageKey, baseTitle = '')", html)
        self.assertIn("function fmtUsdRate(value)", html)
        self.assertIn("String.fromCharCode(10)", html)
        self.assertIn('class="price-tooltip-trigger"', html)
        self.assertIn('tabindex="0"', html)
        self.assertIn("unitPriceSegment", html)
        self.assertIn("cache_write_tokens", html)
        self.assertIn("function loadDailyUsage()", html)
        self.assertIn("/api/daily-usage", html)
        self.assertIn("snapshotToken: ''", html)
        self.assertIn("const nextSnapshotToken = data.snapshot_token || ''", html)
        self.assertIn("state.snapshotToken = nextSnapshotToken", html)
        self.assertIn("const requestedToken = state.snapshotToken", html)
        self.assertIn("params.set('snapshot_token', requestedToken)", html)
        self.assertIn("res.status === 409", html)
        self.assertIn("state.snapshotToken !== requestedToken", html)
        self.assertIn("void showDetails(first.uid)", html)

    def test_html_shows_loading_animation_for_interactive_data_requests(self) -> None:
        html = dashboard.HTML
        overlay_start = html.index('<div class="loading-overlay" id="loadingOverlay"')
        overlay_tag = html[overlay_start:html.index(">", overlay_start)]

        self.assertNotIn(" hidden", overlay_tag)
        self.assertNotIn("aria-busy", overlay_tag)
        self.assertIn('role="status"', overlay_tag)
        self.assertIn('aria-live="polite"', overlay_tag)
        self.assertIn('<main id="dashboardMain" aria-busy="true">', html)
        self.assertIn('class="loading-spinner"', html)
        self.assertIn("@keyframes loading-spin", html)
        self.assertIn(".loading-overlay[hidden]", html)
        self.assertIn("function setLoadingIndicator(active, silent = false)", html)
        self.assertIn("setLoadingIndicator(true, options.silent === true);", html)
        self.assertIn("main.setAttribute('aria-busy', active ? 'true' : 'false');", html)
        self.assertIn("setLoadingIndicator(false);", html)

    def test_http_snapshot_token_serves_consistent_detail_and_rejects_stale_token(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            codex_home = Path(temp_dir) / ".codex"
            self.write_usage_file(
                codex_home,
                "http-snapshot-session",
                100,
                "2026-07-10T10:00:00Z",
            )
            analyzer = dashboard.CodexUsageAnalyzer(codex_home)
            server = dashboard.FixedPortHTTPServer(
                ("127.0.0.1", 0),
                dashboard.make_handler(analyzer),
            )
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            connection = http.client.HTTPConnection(
                "127.0.0.1",
                server.server_address[1],
                timeout=5,
            )
            try:
                connection.request("GET", "/api/sessions?period=all")
                response = connection.getresponse()
                sessions_payload = json.loads(response.read())
                self.assertEqual(response.status, 200)
                token = sessions_payload["snapshot_token"]
                uid = sessions_payload["sessions"][0]["uid"]

                connection.request("GET", "/api/sessions?period=all")
                response = connection.getresponse()
                repeated_payload = json.loads(response.read())
                self.assertEqual(response.status, 200)
                self.assertEqual(repeated_payload, sessions_payload)
                self.assertEqual(len(analyzer._session_payload_cache), 1)

                connection.request(
                    "GET",
                    "/api/session?"
                    + urlencode(
                        {
                            "id": uid,
                            "period": "all",
                            "snapshot_token": token,
                        }
                    ),
                )
                response = connection.getresponse()
                detail = json.loads(response.read())
                self.assertEqual(response.status, 200)
                self.assertEqual(detail["total_token_usage"]["total_tokens"], 100)

                connection.request(
                    "GET",
                    "/api/session?"
                    + urlencode(
                        {
                            "id": uid,
                            "period": "all",
                            "snapshot_token": "not-a-real-snapshot",
                        }
                    ),
                )
                response = connection.getresponse()
                stale = json.loads(response.read())
                self.assertEqual(response.status, 409)
                self.assertEqual(stale["code"], "snapshot_stale")

                connection.request(
                    "GET",
                    "/api/session?" + urlencode({"id": uid, "period": "all"}),
                )
                response = connection.getresponse()
                legacy_detail = json.loads(response.read())
                self.assertEqual(response.status, 200)
                self.assertEqual(legacy_detail["uid"], uid)
            finally:
                connection.close()
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)

    def test_opener_health_check_reads_full_payload(self) -> None:
        class Response:
            status = 200

            def __init__(self, features: list[str]):
                self.features = features

            def __enter__(self) -> "Response":
                return self

            def __exit__(self, *args: object) -> None:
                return None

            def read(self) -> bytes:
                payload = {
                    "ok": True,
                    "app": "codex-usage-dashboard",
                    "padding": "x" * 512,
                    "features": self.features,
                }
                return json.dumps(payload).encode("utf-8")

        original_urlopen = opener.urlopen
        try:
            opener.urlopen = lambda *_args, **_kwargs: Response(dashboard.DASHBOARD_FEATURES)
            self.assertEqual(opener.health_dashboard_url(8765), "http://127.0.0.1:8765/")
            opener.urlopen = lambda *_args, **_kwargs: Response(
                [feature for feature in dashboard.DASHBOARD_FEATURES if feature != "effective-dated-pricing-v1"]
            )
            self.assertIsNone(opener.health_dashboard_url(8765))
            opener.urlopen = lambda *_args, **_kwargs: Response(
                [feature for feature in dashboard.DASHBOARD_FEATURES if feature != "expandable-agent-task-rollups-v1"]
            )
            self.assertIsNone(opener.health_dashboard_url(8765))
            opener.urlopen = lambda *_args, **_kwargs: Response(
                [feature for feature in dashboard.DASHBOARD_FEATURES if feature != "compact-agent-task-tree-v1"]
            )
            self.assertIsNone(opener.health_dashboard_url(8765))
            opener.urlopen = lambda *_args, **_kwargs: Response(
                [feature for feature in dashboard.DASHBOARD_FEATURES if feature != "aligned-agent-task-tree-v1"]
            )
            self.assertIsNone(opener.health_dashboard_url(8765))
            opener.urlopen = lambda *_args, **_kwargs: Response(
                [feature for feature in dashboard.DASHBOARD_FEATURES if feature != "main-agent-child-usage-row-v1"]
            )
            self.assertIsNone(opener.health_dashboard_url(8765))
            opener.urlopen = lambda *_args, **_kwargs: Response(
                [feature for feature in dashboard.DASHBOARD_FEATURES if feature != "aligned-task-title-gutter-v1"]
            )
            self.assertIsNone(opener.health_dashboard_url(8765))
            opener.urlopen = lambda *_args, **_kwargs: Response(
                [feature for feature in dashboard.DASHBOARD_FEATURES if feature != "project-session-indent-v1"]
            )
            self.assertIsNone(opener.health_dashboard_url(8765))
            opener.urlopen = lambda *_args, **_kwargs: Response(
                [feature for feature in dashboard.DASHBOARD_FEATURES if feature != "clickable-task-rows-and-gutter-v1"]
            )
            self.assertIsNone(opener.health_dashboard_url(8765))
            opener.urlopen = lambda *_args, **_kwargs: Response(
                [feature for feature in dashboard.DASHBOARD_FEATURES if feature != "main-agent-title-alignment-v1"]
            )
            self.assertIsNone(opener.health_dashboard_url(8765))
            opener.urlopen = lambda *_args, **_kwargs: Response(
                [feature for feature in dashboard.DASHBOARD_FEATURES if feature != "compact-subagent-indent-v1"]
            )
            self.assertIsNone(opener.health_dashboard_url(8765))
            opener.urlopen = lambda *_args, **_kwargs: Response(
                [feature for feature in dashboard.DASHBOARD_FEATURES if feature != "project-folder-aggregate-columns-v1"]
            )
            self.assertIsNone(opener.health_dashboard_url(8765))
        finally:
            opener.urlopen = original_urlopen

    def test_once_cli_can_build_cold_cache_with_spawn_workers(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            codex_home = root / ".codex"
            for index in range(4):
                self.write_usage_file(
                    codex_home,
                    f"parallel-{index}",
                    100 + index,
                    f"2026-07-10T10:0{index}:00Z",
                )
            env = os.environ.copy()
            env.update(
                {
                    "COUSASH_CONFIG_DIR": str(root / "config"),
                    "COUSASH_PARSE_MIN_FILES": "2",
                    "COUSASH_PARSE_MIN_BYTES": "1",
                    "PYTHONDONTWRITEBYTECODE": "1",
                }
            )
            completed = subprocess.run(
                [
                    os.sys.executable,
                    str(MODULE_PATH),
                    "--codex-home",
                    str(codex_home),
                    "--no-auto-windows",
                    "--parse-workers",
                    "2",
                    "--once",
                    "--json",
                ],
                check=False,
                capture_output=True,
                text=True,
                timeout=30,
                env=env,
            )

        self.assertEqual(completed.returncode, 0, completed.stderr)
        payload = json.loads(completed.stdout)
        self.assertEqual(payload["summary"]["session_count"], 4)
        self.assertEqual(payload["cache_metrics"]["parallel_files"], 4)


if __name__ == "__main__":
    unittest.main()
