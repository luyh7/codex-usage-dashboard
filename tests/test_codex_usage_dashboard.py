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


if __name__ == "__main__":
    unittest.main()
