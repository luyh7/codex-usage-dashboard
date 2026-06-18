import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


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


class CodexUsageDashboardTests(unittest.TestCase):
    def write_usage_file(
        self,
        codex_home: Path,
        session_id: str,
        total_tokens: int,
        timestamp: str,
        cwd: str | None = None,
    ) -> Path:
        sessions_dir = codex_home / "sessions"
        sessions_dir.mkdir(parents=True)
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
        self.assertIn("function folderIcon()", html)
        self.assertIn("compactProject", html)
        self.assertIn("archivedDelta", html)
        self.assertLess(html.index("${folderIcon()}"), html.index('<span class="project-name"'))
        self.assertLess(html.index('<div class="project-title-cell">'), html.index("${environmentBadge(group)}"))
        self.assertLess(html.index("${environmentBadge(group)}"), html.index('<div class="project-meta">'))


if __name__ == "__main__":
    unittest.main()
