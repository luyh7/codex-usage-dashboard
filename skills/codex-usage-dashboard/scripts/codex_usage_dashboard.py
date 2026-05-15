#!/usr/bin/env python3
"""Local read-only dashboard for Codex session token usage.

The server reads Codex JSONL logs from:
  - ~/.codex/sessions
  - ~/.codex/archived_sessions

It does not modify Codex files. Bind address defaults to 127.0.0.1.
"""

from __future__ import annotations

import argparse
import csv
import datetime as dt
import hashlib
import io
import json
import os
import re
import socket
import sys
import threading
import time
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.request import urlopen
from urllib.parse import parse_qs, urlparse


TOKEN_KEYS = (
    "input_tokens",
    "cached_input_tokens",
    "output_tokens",
    "reasoning_output_tokens",
    "total_tokens",
)

MODEL_PRICES_USD_PER_M_TOKENS = {
    "gpt-5.5": {"input": 5.00, "cached_input": 0.50, "output": 30.00},
    "gpt-5.4": {"input": 2.50, "cached_input": 0.25, "output": 15.00},
    "gpt-5.4-mini": {"input": 0.75, "cached_input": 0.075, "output": 4.50},
    "gpt-5.3-codex": {"input": 1.75, "cached_input": 0.175, "output": 14.00},
    "gpt-5.3-chat-latest": {"input": 1.75, "cached_input": 0.175, "output": 14.00},
    "gpt-5.2": {"input": 1.75, "cached_input": 0.175, "output": 14.00},
    "gpt-5.2-codex": {"input": 1.75, "cached_input": 0.175, "output": 14.00},
    "gpt-5.2-chat-latest": {"input": 1.75, "cached_input": 0.175, "output": 14.00},
    "gpt-5.1-codex": {"input": 1.25, "cached_input": 0.125, "output": 10.00},
    "gpt-5.1-codex-max": {"input": 1.25, "cached_input": 0.125, "output": 10.00},
    "gpt-5": {"input": 1.25, "cached_input": 0.125, "output": 10.00},
    "gpt-5-codex": {"input": 1.25, "cached_input": 0.125, "output": 10.00},
}


def zero_usage() -> dict[str, int]:
    return {key: 0 for key in TOKEN_KEYS}


def normalize_usage(value: Any) -> dict[str, int]:
    usage = zero_usage()
    if isinstance(value, dict):
        for key in TOKEN_KEYS:
            raw = value.get(key, 0)
            if isinstance(raw, (int, float)):
                usage[key] = int(raw)
    return usage


def add_usage(left: dict[str, int], right: dict[str, int]) -> dict[str, int]:
    return {key: int(left.get(key, 0)) + int(right.get(key, 0)) for key in TOKEN_KEYS}


def rate_for_model(model: str, rates: dict[str, dict[str, float]]) -> dict[str, float] | None:
    model_key = (model or "").lower()
    if model_key in rates:
        return rates[model_key]
    for key in sorted(rates, key=len, reverse=True):
        if model_key.startswith(key):
            return rates[key]
    return None


def price_for_model(model: str) -> dict[str, float] | None:
    return rate_for_model(model, MODEL_PRICES_USD_PER_M_TOKENS)


def estimate_cost_usd(usage: dict[str, int], model: str) -> float | None:
    price = price_for_model(model)
    if not price:
        return None
    input_tokens = max(int(usage.get("input_tokens", 0)), 0)
    cached_tokens = min(max(int(usage.get("cached_input_tokens", 0)), 0), input_tokens)
    uncached_tokens = input_tokens - cached_tokens
    output_tokens = max(int(usage.get("output_tokens", 0)), 0)
    cost = (
        uncached_tokens * price["input"]
        + cached_tokens * price["cached_input"]
        + output_tokens * price["output"]
    ) / 1_000_000
    return round(cost, 6)


def estimate_cost_breakdown_usd(usage: dict[str, int], model: str) -> dict[str, float] | None:
    price = price_for_model(model)
    if not price:
        return None
    input_tokens = max(int(usage.get("input_tokens", 0)), 0)
    cached_tokens = min(max(int(usage.get("cached_input_tokens", 0)), 0), input_tokens)
    uncached_tokens = input_tokens - cached_tokens
    output_tokens = max(int(usage.get("output_tokens", 0)), 0)
    reasoning_tokens = min(max(int(usage.get("reasoning_output_tokens", 0)), 0), output_tokens)
    return {
        "input_tokens": round(uncached_tokens * price["input"] / 1_000_000, 6),
        "cached_input_tokens": round(cached_tokens * price["cached_input"] / 1_000_000, 6),
        "output_tokens": round(output_tokens * price["output"] / 1_000_000, 6),
        "reasoning_output_tokens": round(reasoning_tokens * price["output"] / 1_000_000, 6),
    }


def utc_from_epoch(seconds: float | int | None) -> str | None:
    if seconds is None:
        return None
    try:
        return dt.datetime.fromtimestamp(float(seconds), tz=dt.UTC).isoformat().replace("+00:00", "Z")
    except (TypeError, ValueError, OSError):
        return None


def utc_from_mtime(path: Path) -> str | None:
    try:
        return utc_from_epoch(path.stat().st_mtime)
    except OSError:
        return None


def clean_text(value: str, limit: int = 260) -> str:
    text = re.sub(r"\s+", " ", value).strip()
    if len(text) > limit:
        return text[: limit - 1].rstrip() + "…"
    return text


def safe_print(*values: Any) -> None:
    try:
        if sys.stdout is not None and not sys.stdout.closed:
            print(*values)
    except Exception:
        pass


def text_from_content(content: Any) -> str:
    if isinstance(content, str):
        return content
    if isinstance(content, dict):
        for key in ("text", "message", "content"):
            if isinstance(content.get(key), str):
                return content[key]
        return ""
    if isinstance(content, list):
        parts: list[str] = []
        for item in content:
            if isinstance(item, str):
                parts.append(item)
            elif isinstance(item, dict):
                for key in ("text", "message", "content"):
                    value = item.get(key)
                    if isinstance(value, str):
                        parts.append(value)
                        break
        return "\n".join(parts)
    return ""


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8-sig", errors="replace") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                rows.append({"__parse_error__": True})
                continue
            if isinstance(item, dict):
                rows.append(item)
    return rows


class CodexUsageAnalyzer:
    def __init__(self, codex_home: Path):
        self.codex_home = codex_home.expanduser().resolve()
        self._cache: dict[str, tuple[int, int, dict[str, Any], dict[str, Any]]] = {}

    def load_session_titles(self) -> dict[str, str]:
        titles: dict[str, str] = {}
        index_path = self.codex_home / "session_index.jsonl"
        if not index_path.exists():
            return titles

        try:
            for item in read_jsonl(index_path):
                session_id = item.get("id")
                title = item.get("thread_name")
                if isinstance(session_id, str) and isinstance(title, str) and title.strip():
                    titles[session_id] = clean_text(title, 180)
        except OSError:
            return titles
        return titles

    def iter_session_files(self) -> list[tuple[Path, str]]:
        files: list[tuple[Path, str]] = []
        sessions_dir = self.codex_home / "sessions"
        archived_dir = self.codex_home / "archived_sessions"

        if sessions_dir.exists():
            for path in sessions_dir.rglob("*.jsonl"):
                files.append((path, "active"))
        if archived_dir.exists():
            for path in archived_dir.glob("*.jsonl"):
                files.append((path, "archived"))

        files.sort(key=lambda item: item[0].stat().st_mtime if item[0].exists() else 0, reverse=True)
        return files

    def scan(self) -> dict[str, Any]:
        titles = self.load_session_titles()
        sessions: list[dict[str, Any]] = []
        details_by_uid: dict[str, dict[str, Any]] = {}

        for path, source in self.iter_session_files():
            try:
                summary, detail = self.parse_file_cached(path, source)
            except OSError:
                continue

            session_id = summary.get("session_id")
            if isinstance(session_id, str) and session_id in titles:
                summary["title"] = titles[session_id]
                detail["title"] = titles[session_id]

            sessions.append(summary)
            details_by_uid[summary["uid"]] = detail

        sessions.sort(key=lambda row: row["total_token_usage"].get("total_tokens", 0), reverse=True)
        return {
            "generated_at": dt.datetime.now(dt.UTC).isoformat().replace("+00:00", "Z"),
            "codex_home": str(self.codex_home),
            "sessions": sessions,
            "details_by_uid": details_by_uid,
            "summary": self.build_summary(sessions),
        }

    def parse_file_cached(self, path: Path, source: str) -> tuple[dict[str, Any], dict[str, Any]]:
        stat = path.stat()
        cache_key = str(path.resolve())
        cached = self._cache.get(cache_key)
        if cached and cached[0] == stat.st_mtime_ns and cached[1] == stat.st_size:
            return cached[2], cached[3]

        summary, detail = self.parse_file(path, source)
        self._cache[cache_key] = (stat.st_mtime_ns, stat.st_size, summary, detail)
        return summary, detail

    def parse_file(self, path: Path, source: str) -> tuple[dict[str, Any], dict[str, Any]]:
        uid = hashlib.sha1(str(path.resolve()).encode("utf-8", errors="replace")).hexdigest()[:16]
        session_id = ""
        title = ""
        cwd = ""
        model = ""
        effort = ""
        originator = ""
        cli_version = ""
        created_at = ""
        start_at = ""
        end_at = ""
        model_context_window: int | None = None
        latest_rate_limits: dict[str, Any] | None = None
        latest_rate_limit_reached_type: str | None = None
        latest_plan_type: str | None = None

        total_usage = zero_usage()
        last_usage = zero_usage()
        timeline: list[dict[str, Any]] = []
        tasks: list[dict[str, Any]] = []
        tool_counts: dict[str, int] = {}
        turn_ids: set[str] = set()
        parse_errors = 0
        line_count = 0
        token_event_count = 0
        user_message_count = 0
        assistant_message_count = 0
        first_user_prompt = ""
        last_agent_preview = ""
        durations_ms: list[int] = []
        ttf_ms: list[int] = []

        rows = read_jsonl(path)
        for item in rows:
            line_count += 1
            if item.get("__parse_error__"):
                parse_errors += 1
                continue

            timestamp = item.get("timestamp")
            if isinstance(timestamp, str):
                if not start_at or timestamp < start_at:
                    start_at = timestamp
                if not end_at or timestamp > end_at:
                    end_at = timestamp

            item_type = item.get("type")
            payload = item.get("payload") if isinstance(item.get("payload"), dict) else {}

            if item_type == "session_meta":
                session_id = str(payload.get("id") or session_id)
                cwd = str(payload.get("cwd") or cwd)
                created_at = str(payload.get("timestamp") or created_at)
                originator = str(payload.get("originator") or originator)
                cli_version = str(payload.get("cli_version") or cli_version)
                model = str(payload.get("model") or payload.get("model_slug") or model)

            elif item_type == "turn_context":
                turn_id = payload.get("turn_id")
                if isinstance(turn_id, str):
                    turn_ids.add(turn_id)
                cwd = str(payload.get("cwd") or cwd)
                model = str(payload.get("model") or model)
                effort = str(payload.get("effort") or effort)

            elif item_type == "response_item":
                response_type = payload.get("type")
                if response_type == "message":
                    role = payload.get("role")
                    text = clean_text(text_from_content(payload.get("content")), 260)
                    if role == "user":
                        user_message_count += 1
                        if text and not first_user_prompt:
                            first_user_prompt = text
                    elif role == "assistant":
                        assistant_message_count += 1
                        if text:
                            last_agent_preview = text
                elif response_type == "function_call":
                    name = str(payload.get("name") or "function_call")
                    tool_counts[name] = tool_counts.get(name, 0) + 1

            elif item_type == "event_msg":
                event_type = payload.get("type")
                if event_type == "token_count":
                    latest_rate_limits = payload.get("rate_limits") if isinstance(payload.get("rate_limits"), dict) else None
                    latest_plan_type = payload.get("plan_type") if isinstance(payload.get("plan_type"), str) else latest_plan_type
                    reached = payload.get("rate_limit_reached_type")
                    latest_rate_limit_reached_type = reached if isinstance(reached, str) else latest_rate_limit_reached_type

                    info = payload.get("info")
                    if isinstance(info, dict):
                        token_event_count += 1
                        total_usage = normalize_usage(info.get("total_token_usage"))
                        last_usage = normalize_usage(info.get("last_token_usage"))
                        window = info.get("model_context_window")
                        if isinstance(window, (int, float)):
                            model_context_window = int(window)
                        timeline.append(
                            {
                                "timestamp": timestamp,
                                "total_token_usage": total_usage,
                                "last_token_usage": last_usage,
                                "model_context_window": model_context_window,
                                "rate_limits": latest_rate_limits,
                            }
                        )

                elif event_type == "task_complete":
                    duration = payload.get("duration_ms")
                    first_token = payload.get("time_to_first_token_ms")
                    turn_id = payload.get("turn_id")
                    if isinstance(turn_id, str):
                        turn_ids.add(turn_id)
                    if isinstance(duration, (int, float)):
                        durations_ms.append(int(duration))
                    if isinstance(first_token, (int, float)):
                        ttf_ms.append(int(first_token))
                    tasks.append(
                        {
                            "timestamp": timestamp,
                            "turn_id": turn_id,
                            "duration_ms": int(duration) if isinstance(duration, (int, float)) else None,
                            "time_to_first_token_ms": int(first_token) if isinstance(first_token, (int, float)) else None,
                        }
                    )
                    last_message = payload.get("last_agent_message")
                    if isinstance(last_message, str):
                        last_agent_preview = clean_text(last_message, 320)

                elif event_type in {"agent_message", "assistant_message"}:
                    assistant_message_count += 1
                    message = payload.get("message")
                    if isinstance(message, str):
                        last_agent_preview = clean_text(message, 320)

                elif event_type in {"user_message", "user_input", "human_message"}:
                    user_message_count += 1
                    message = payload.get("message") or payload.get("text") or payload.get("content")
                    if isinstance(message, str) and not first_user_prompt:
                        first_user_prompt = clean_text(message, 260)

        if not session_id:
            match = re.search(r"rollout-[^-]+-[^-]+-(.+?)\.jsonl$", path.name)
            session_id = match.group(1) if match else uid

        if not title:
            title = first_user_prompt or Path(cwd).name or path.stem

        if not start_at:
            start_at = created_at or utc_from_mtime(path) or ""
        if not end_at:
            end_at = utc_from_mtime(path) or start_at

        cached_percent = None
        input_tokens = total_usage.get("input_tokens", 0)
        if input_tokens:
            cached_percent = round(total_usage.get("cached_input_tokens", 0) / input_tokens * 100, 1)
        estimated_cost_usd = estimate_cost_usd(total_usage, model)
        estimated_cost_breakdown_usd = estimate_cost_breakdown_usd(total_usage, model)

        detail: dict[str, Any] = {
            "uid": uid,
            "session_id": session_id,
            "title": title,
            "source": source,
            "path": str(path.resolve()),
            "file_size": path.stat().st_size,
            "line_count": line_count,
            "parse_errors": parse_errors,
            "created_at": created_at or start_at,
            "start_at": start_at,
            "end_at": end_at,
            "updated_at": utc_from_mtime(path),
            "cwd": cwd,
            "project": Path(cwd).name if cwd else "",
            "model": model,
            "effort": effort,
            "originator": originator,
            "cli_version": cli_version,
            "total_token_usage": total_usage,
            "last_token_usage": last_usage,
            "estimated_cost_usd": estimated_cost_usd,
            "estimated_cost_breakdown_usd": estimated_cost_breakdown_usd,
            "price_model_known": estimated_cost_usd is not None,
            "cached_input_percent": cached_percent,
            "model_context_window": model_context_window,
            "token_event_count": token_event_count,
            "turn_count": len(turn_ids) or len(tasks) or token_event_count,
            "completed_turn_count": len(tasks),
            "user_message_count": user_message_count,
            "assistant_message_count": assistant_message_count,
            "first_user_prompt": first_user_prompt,
            "last_agent_preview": last_agent_preview,
            "latest_rate_limits": latest_rate_limits,
            "latest_plan_type": latest_plan_type,
            "latest_rate_limit_reached_type": latest_rate_limit_reached_type,
            "tool_counts": dict(sorted(tool_counts.items(), key=lambda item: item[1], reverse=True)),
            "duration_ms_total": sum(durations_ms),
            "duration_ms_avg": int(sum(durations_ms) / len(durations_ms)) if durations_ms else None,
            "time_to_first_token_ms_avg": int(sum(ttf_ms) / len(ttf_ms)) if ttf_ms else None,
            "timeline": timeline,
            "tasks": tasks,
        }

        summary_keys = {
            "uid",
            "session_id",
            "title",
            "source",
            "path",
            "file_size",
            "parse_errors",
            "created_at",
            "start_at",
            "end_at",
            "updated_at",
            "cwd",
            "project",
            "model",
            "effort",
            "total_token_usage",
            "last_token_usage",
            "estimated_cost_usd",
            "estimated_cost_breakdown_usd",
            "price_model_known",
            "cached_input_percent",
            "token_event_count",
            "turn_count",
            "completed_turn_count",
            "duration_ms_total",
            "time_to_first_token_ms_avg",
        }
        summary = {key: detail[key] for key in summary_keys}
        return summary, detail

    def build_summary(self, sessions: list[dict[str, Any]]) -> dict[str, Any]:
        totals = zero_usage()
        by_model: dict[str, dict[str, Any]] = {}
        by_project: dict[str, dict[str, Any]] = {}
        by_day: dict[str, dict[str, Any]] = {}
        active_count = 0
        archived_count = 0
        estimated_cost_total = 0.0
        estimated_cost_known_count = 0

        for session in sessions:
            usage = normalize_usage(session.get("total_token_usage"))
            totals = add_usage(totals, usage)
            if isinstance(session.get("estimated_cost_usd"), (int, float)):
                estimated_cost_total += float(session["estimated_cost_usd"])
                estimated_cost_known_count += 1

            if session.get("source") == "active":
                active_count += 1
            elif session.get("source") == "archived":
                archived_count += 1

            model = str(session.get("model") or "unknown")
            by_model.setdefault(model, {"model": model, "sessions": 0, "usage": zero_usage()})
            by_model[model]["sessions"] += 1
            by_model[model]["usage"] = add_usage(by_model[model]["usage"], usage)

            project = str(session.get("project") or "unknown")
            by_project.setdefault(project, {"project": project, "sessions": 0, "usage": zero_usage()})
            by_project[project]["sessions"] += 1
            by_project[project]["usage"] = add_usage(by_project[project]["usage"], usage)

            stamp = str(session.get("end_at") or session.get("start_at") or "")[:10] or "unknown"
            by_day.setdefault(stamp, {"date": stamp, "sessions": 0, "usage": zero_usage()})
            by_day[stamp]["sessions"] += 1
            by_day[stamp]["usage"] = add_usage(by_day[stamp]["usage"], usage)

        return {
            "session_count": len(sessions),
            "active_count": active_count,
            "archived_count": archived_count,
            "usage": totals,
            "estimated_cost_usd": round(estimated_cost_total, 6),
            "estimated_cost_known_count": estimated_cost_known_count,
            "by_model": sorted(by_model.values(), key=lambda row: row["usage"]["total_tokens"], reverse=True),
            "by_project": sorted(by_project.values(), key=lambda row: row["usage"]["total_tokens"], reverse=True)[:20],
            "by_day": sorted(by_day.values(), key=lambda row: row["date"]),
            "top_session_uid": sessions[0]["uid"] if sessions else None,
        }

    def get_detail(self, uid: str) -> dict[str, Any] | None:
        snapshot = self.scan()
        return snapshot["details_by_uid"].get(uid)

    def export_csv(self) -> str:
        snapshot = self.scan()
        buffer = io.StringIO()
        writer = csv.writer(buffer)
        writer.writerow(
            [
                "rank",
                "title",
                "session_id",
                "source",
                "model",
                "project",
                "start_at",
                "end_at",
                "total_tokens",
                "estimated_cost_usd",
                "cached_input_percent",
                "input_tokens",
                "cached_input_tokens",
                "output_tokens",
                "reasoning_output_tokens",
                "turn_count",
                "completed_turn_count",
                "effort",
                "cwd",
                "path",
            ]
        )
        for index, session in enumerate(snapshot["sessions"], start=1):
            usage = normalize_usage(session.get("total_token_usage"))
            writer.writerow(
                [
                    index,
                    session.get("title", ""),
                    session.get("session_id", ""),
                    session.get("source", ""),
                    session.get("model", ""),
                    session.get("project", ""),
                    session.get("start_at", ""),
                    session.get("end_at", ""),
                    usage["total_tokens"],
                    session.get("estimated_cost_usd", ""),
                    session.get("cached_input_percent", ""),
                    usage["input_tokens"],
                    usage["cached_input_tokens"],
                    usage["output_tokens"],
                    usage["reasoning_output_tokens"],
                    session.get("turn_count", ""),
                    session.get("completed_turn_count", ""),
                    session.get("effort", ""),
                    session.get("cwd", ""),
                    session.get("path", ""),
                ]
            )
        return buffer.getvalue()


HTML = r"""<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Codex Usage Dashboard</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f5f7f6;
      --panel: #ffffff;
      --line: #d9dfdd;
      --text: #17201d;
      --muted: #65716c;
      --accent: #0f7b63;
      --accent-2: #b85f18;
      --accent-3: #2d5fa8;
      --danger: #b42318;
      --soft: #edf4f1;
      --shadow: 0 10px 30px rgba(26, 36, 32, 0.08);
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background: var(--bg);
      color: var(--text);
      font-size: 14px;
    }
    header {
      position: sticky;
      top: 0;
      z-index: 5;
      background: rgba(245, 247, 246, 0.94);
      border-bottom: 1px solid var(--line);
      backdrop-filter: blur(10px);
    }
    .header-inner {
      max-width: 1480px;
      margin: 0 auto;
      padding: 18px 24px 14px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
    }
    h1 {
      margin: 0;
      font-size: 22px;
      line-height: 1.2;
      letter-spacing: 0;
    }
    .subtitle {
      margin-top: 4px;
      color: var(--muted);
      font-size: 13px;
      word-break: break-all;
    }
    .toolbar {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
      justify-content: flex-end;
    }
    button, select, input {
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #fff;
      color: var(--text);
      min-height: 36px;
      font: inherit;
    }
    button {
      padding: 0 12px;
      cursor: pointer;
    }
    button.primary {
      background: var(--accent);
      border-color: var(--accent);
      color: #fff;
    }
    button:hover { border-color: #9da9a4; }
    main {
      max-width: 1480px;
      margin: 0 auto;
      padding: 20px 24px 28px;
    }
    .metrics {
      display: grid;
      grid-template-columns: repeat(6, minmax(130px, 1fr));
      gap: 12px;
      margin-bottom: 16px;
    }
    .metric {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 14px;
      box-shadow: var(--shadow);
      min-width: 0;
    }
    .metric .label {
      color: var(--muted);
      font-size: 12px;
      margin-bottom: 6px;
      white-space: nowrap;
    }
    .metric .value {
      font-size: 22px;
      font-weight: 700;
      line-height: 1.1;
      overflow-wrap: anywhere;
    }
    .metric .hint {
      color: var(--muted);
      font-size: 12px;
      margin-top: 5px;
      min-height: 16px;
      overflow-wrap: anywhere;
    }
    .controls {
      display: grid;
      grid-template-columns: minmax(220px, 1fr) 150px 170px 210px 130px;
      gap: 10px;
      margin-bottom: 16px;
    }
    input, select {
      width: 100%;
      padding: 0 10px;
    }
    .sort-toggle {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 4px;
      min-height: 36px;
      padding: 3px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #fff;
    }
    .sort-option {
      min-height: 28px;
      border: 0;
      border-radius: 6px;
      background: transparent;
      color: var(--muted);
      font-weight: 700;
      padding: 0 10px;
    }
    .sort-option.active {
      background: var(--accent);
      color: #fff;
      box-shadow: 0 1px 4px rgba(15, 123, 99, 0.22);
    }
    .layout {
      display: grid;
      grid-template-columns: minmax(0, 1.8fr) minmax(360px, 0.9fr);
      gap: 16px;
      align-items: start;
    }
    section, aside {
      min-width: 0;
    }
    .panel {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      box-shadow: var(--shadow);
    }
    .panel-title {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 14px 14px 10px;
      border-bottom: 1px solid var(--line);
    }
    .panel-title h2 {
      margin: 0;
      font-size: 15px;
      letter-spacing: 0;
    }
    .count {
      color: var(--muted);
      font-size: 12px;
      white-space: nowrap;
    }
    .chart {
      padding: 12px 14px 4px;
      border-bottom: 1px solid var(--line);
    }
    .bar-row {
      display: grid;
      grid-template-columns: minmax(100px, 1fr) minmax(120px, 2fr) 90px;
      gap: 10px;
      align-items: center;
      margin-bottom: 8px;
      color: var(--muted);
      font-size: 12px;
    }
    .bar-label {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .bar-track {
      height: 8px;
      border-radius: 6px;
      background: #edf0ef;
      overflow: hidden;
    }
    .bar-fill {
      height: 100%;
      border-radius: 6px;
      background: var(--accent);
      min-width: 2px;
    }
    .table-wrap {
      overflow: auto;
      max-height: calc(100vh - 270px);
    }
    table {
      width: 100%;
      border-collapse: collapse;
      table-layout: fixed;
      min-width: 830px;
    }
    th, td {
      border-bottom: 1px solid var(--line);
      padding: 9px 8px;
      text-align: left;
      vertical-align: middle;
    }
    th {
      position: sticky;
      top: 0;
      background: #fbfcfc;
      color: var(--muted);
      z-index: 1;
      font-size: 12px;
      font-weight: 650;
      cursor: pointer;
      white-space: nowrap;
    }
    td {
      font-size: 13px;
    }
    .col-title { width: 238px; }
    .col-total { width: 86px; }
    .col-output { width: 78px; }
    .col-cost { width: 72px; }
    .col-cache { width: 82px; }
    .col-turns { width: 58px; }
    .col-model { width: 112px; }
    .col-effort { width: 76px; }
    th[data-sort="total_tokens"],
    th[data-sort="output_tokens"],
    th[data-sort="estimated_cost_usd"],
    th[data-sort="cached_input_percent"],
    th[data-sort="turn_count"] {
      text-align: right;
    }
    th[data-sort="model"],
    th[data-sort="effort"] {
      overflow: hidden;
      text-overflow: ellipsis;
    }
    tr:hover td { background: #f7faf9; }
    tr.selected td { background: var(--soft); }
    .rank {
      color: var(--muted);
      white-space: nowrap;
      text-align: center;
      padding-left: 4px;
      padding-right: 4px;
    }
    .title-cell {
      min-width: 0;
    }
    .title-line {
      display: flex;
      align-items: center;
      gap: 8px;
      min-width: 0;
    }
    .title-main {
      font-weight: 650;
      margin-bottom: 4px;
      line-height: 1.35;
      min-width: 0;
      flex: 1 1 auto;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .title-time {
      flex: 0 0 auto;
      color: var(--muted);
      font-size: 12px;
      font-weight: 650;
      white-space: nowrap;
    }
    .title-sub {
      color: var(--muted);
      font-size: 12px;
      display: flex;
      align-items: center;
      gap: 6px;
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .title-sub-text {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .number {
      font-variant-numeric: tabular-nums;
      text-align: right;
      white-space: nowrap;
    }
    .model-cell,
    .effort-cell {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .badge {
      display: inline-flex;
      align-items: center;
      flex: 0 0 auto;
      min-height: 22px;
      padding: 2px 7px;
      border-radius: 999px;
      border: 1px solid var(--line);
      background: #fff;
      color: var(--muted);
      font-size: 12px;
      white-space: nowrap;
    }
    .badge.active { color: var(--accent); border-color: #99c9bb; background: #eef8f4; }
    .badge.archived { color: var(--accent-2); border-color: #e3b887; background: #fff6ec; }
    .details {
      position: sticky;
      top: 94px;
      max-height: calc(100vh - 112px);
      overflow: auto;
    }
    .details-body {
      padding: 14px;
    }
    .empty {
      color: var(--muted);
      padding: 22px 14px;
      text-align: center;
    }
    .detail-title {
      font-size: 17px;
      font-weight: 750;
      line-height: 1.35;
      margin-bottom: 8px;
      overflow-wrap: anywhere;
    }
    .detail-meta {
      display: grid;
      gap: 8px;
      margin: 12px 0;
    }
    .kv {
      display: grid;
      grid-template-columns: 110px minmax(0, 1fr);
      gap: 10px;
      color: var(--muted);
      font-size: 12px;
    }
    .kv strong {
      color: var(--text);
      font-weight: 550;
      overflow-wrap: anywhere;
    }
    .breakdown {
      display: grid;
      gap: 8px;
      margin: 14px 0;
    }
    .breakdown-row {
      display: grid;
      grid-template-columns: 92px minmax(100px, 1fr) 148px;
      gap: 8px;
      align-items: center;
      font-size: 12px;
      color: var(--muted);
    }
    .breakdown-value {
      display: flex;
      justify-content: flex-end;
      gap: 8px;
      white-space: nowrap;
    }
    .breakdown-value strong {
      color: var(--text);
      font-weight: 650;
    }
    .breakdown-row .bar-fill.input { background: var(--accent-3); }
    .breakdown-row .bar-fill.cached { background: var(--accent); }
    .breakdown-row .bar-fill.output { background: var(--accent-2); }
    .breakdown-row .bar-fill.reasoning { background: #6f6a25; }
    canvas {
      width: 100%;
      height: 150px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #fbfcfc;
      display: block;
    }
    .section-label {
      margin: 16px 0 8px;
      color: var(--muted);
      font-size: 12px;
      font-weight: 700;
      text-transform: uppercase;
    }
    .mini-table {
      min-width: 0;
      font-size: 12px;
    }
    .mini-table th, .mini-table td {
      padding: 7px 8px;
    }
    .mini-table th {
      position: static;
      cursor: default;
    }
    .path {
      font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
      font-size: 12px;
      overflow-wrap: anywhere;
    }
    .notice {
      margin-top: 10px;
      color: var(--muted);
      font-size: 12px;
      line-height: 1.5;
    }
    .error {
      color: var(--danger);
      padding: 14px;
    }
    @media (max-width: 1100px) {
      .metrics { grid-template-columns: repeat(3, minmax(130px, 1fr)); }
      .layout { grid-template-columns: 1fr; }
      .details { position: static; max-height: none; }
      .table-wrap { max-height: none; }
    }
    @media (max-width: 760px) {
      .header-inner { align-items: flex-start; flex-direction: column; padding: 16px; }
      main { padding: 16px; }
      .toolbar { justify-content: flex-start; }
      .metrics { grid-template-columns: repeat(2, minmax(120px, 1fr)); }
      .controls { grid-template-columns: 1fr; }
      .bar-row { grid-template-columns: 1fr; gap: 4px; }
      .kv { grid-template-columns: 1fr; gap: 2px; }
    }
  </style>
</head>
<body>
  <header>
    <div class="header-inner">
      <div>
        <h1>Codex Usage Dashboard</h1>
        <div class="subtitle" id="codexHome">读取本地 Codex 会话日志</div>
      </div>
      <div class="toolbar">
        <button id="refreshBtn" class="primary" title="重新扫描本地日志">刷新</button>
        <button id="exportBtn" title="导出当前所有会话统计">导出 CSV</button>
      </div>
    </div>
  </header>

  <main>
    <div class="metrics" id="metrics"></div>

    <div class="controls">
      <input id="searchInput" type="search" placeholder="搜索标题、项目、路径、模型">
      <select id="sourceFilter" title="来源">
        <option value="all">全部来源</option>
        <option value="active">当前会话</option>
        <option value="archived">归档会话</option>
      </select>
      <select id="modelFilter" title="模型">
        <option value="all">全部模型</option>
      </select>
      <div class="sort-toggle" aria-label="排序">
        <button class="sort-option active" data-sort-button="end_at" type="button">最近</button>
        <button class="sort-option" data-sort-button="total_tokens" type="button">总量</button>
      </div>
      <select id="limitSelect" title="显示数量">
        <option value="50">前 50</option>
        <option value="100">前 100</option>
        <option value="all">全部</option>
      </select>
    </div>

    <div class="layout">
      <section class="panel">
        <div class="panel-title">
          <h2>对话</h2>
          <div class="count" id="resultCount">加载中</div>
        </div>
        <div class="table-wrap">
          <table>
            <colgroup>
              <col class="col-title">
              <col class="col-total">
              <col class="col-output">
              <col class="col-cost">
              <col class="col-cache">
              <col class="col-turns">
              <col class="col-model">
              <col class="col-effort">
            </colgroup>
            <thead>
              <tr>
                <th data-sort="title">对话</th>
                <th data-sort="total_tokens">总量</th>
                <th data-sort="output_tokens">输出</th>
                <th data-sort="estimated_cost_usd">花费</th>
                <th data-sort="cached_input_percent">缓存命中</th>
                <th data-sort="turn_count">轮次</th>
                <th data-sort="model">模型</th>
                <th data-sort="effort">推理强度</th>
              </tr>
            </thead>
            <tbody id="sessionRows">
              <tr><td colspan="8" class="empty">加载中</td></tr>
            </tbody>
          </table>
        </div>
      </section>

      <aside class="panel details">
        <div class="panel-title">
          <h2>对话明细</h2>
          <div class="count" id="detailStatus">未选择</div>
        </div>
        <div class="details-body" id="detailsBody">
          <div class="empty">点击左侧任意一行查看 token 明细和时间线。</div>
        </div>
      </aside>
    </div>
  </main>

  <script>
    const state = {
      sessions: [],
      summary: null,
      selectedUid: null,
      sortKey: 'end_at',
      sortDir: 'desc',
      search: '',
      source: 'all',
      model: 'all',
      limit: '50',
    };

    const tokenKeys = ['input_tokens', 'cached_input_tokens', 'output_tokens', 'reasoning_output_tokens', 'total_tokens'];

    function usageOf(row) {
      return row && row.total_token_usage ? row.total_token_usage : {};
    }

    function tokenValue(row, key) {
      return Number(usageOf(row)[key] || 0);
    }

    function fmtPercent(value) {
      if (value === null || value === undefined || value === '') return 'N/A';
      const number = Number(value);
      if (!Number.isFinite(number)) return 'N/A';
      return number.toFixed(1) + '%';
    }

    function fmtUsd(value) {
      if (value === null || value === undefined || value === '') return 'N/A';
      const number = Number(value);
      if (!Number.isFinite(number)) return 'N/A';
      if (number > 0 && number < 0.01) return '$' + number.toFixed(4);
      return '$' + number.toFixed(2);
    }

    function fmt(n) {
      return Number(n || 0).toLocaleString('zh-CN');
    }

    function fmtCompact(n) {
      const value = Number(n || 0);
      if (value >= 1000000000) return (value / 1000000000).toFixed(2) + 'B';
      if (value >= 1000000) return (value / 1000000).toFixed(2) + 'M';
      if (value >= 1000) return (value / 1000).toFixed(1) + 'K';
      return String(value);
    }

    function fmtDate(value) {
      if (!value) return '';
      const date = new Date(value);
      if (Number.isNaN(date.getTime())) return String(value);
      return date.toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
    }

    function fmtRelativeTime(value) {
      if (!value) return '';
      const date = new Date(value);
      if (Number.isNaN(date.getTime())) return '';
      const diffSeconds = Math.max(0, Math.floor((Date.now() - date.getTime()) / 1000));
      if (diffSeconds < 60) return '刚刚';
      const minutes = Math.floor(diffSeconds / 60);
      if (minutes < 60) return minutes + ' 分钟';
      const hours = Math.floor(minutes / 60);
      if (hours < 48) return hours + ' 小时';
      const days = Math.floor(hours / 24);
      if (days < 30) return days + ' 天';
      const months = Math.floor(days / 30);
      if (months < 12) return months + ' 个月';
      return Math.floor(months / 12) + ' 年';
    }

    function fmtDuration(ms) {
      if (!ms) return '';
      const seconds = Math.round(ms / 1000);
      if (seconds < 60) return seconds + ' 秒';
      const minutes = Math.floor(seconds / 60);
      const rest = seconds % 60;
      if (minutes < 60) return minutes + ' 分 ' + rest + ' 秒';
      const hours = Math.floor(minutes / 60);
      return hours + ' 小时 ' + (minutes % 60) + ' 分';
    }

    function escapeHtml(value) {
      return String(value ?? '')
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;')
        .replaceAll("'", '&#39;');
    }

    function shortPath(path) {
      if (!path) return '';
      const parts = String(path).split(/[\\/]+/).filter(Boolean);
      return parts.slice(-3).join(' / ');
    }

    function sourceBadge(source) {
      if (source !== 'archived') return '';
      const text = '归档';
      return `<span class="badge ${escapeHtml(source)}">${text}</span>`;
    }

    async function loadData(selectTop = true) {
      const refreshBtn = document.getElementById('refreshBtn');
      refreshBtn.disabled = true;
      refreshBtn.textContent = '扫描中';
      try {
        const res = await fetch('/api/sessions', { cache: 'no-store' });
        if (!res.ok) throw new Error('HTTP ' + res.status);
        const data = await res.json();
        state.sessions = data.sessions || [];
        state.summary = data.summary || null;
        document.getElementById('codexHome').textContent = `${data.codex_home || ''} · ${fmtDate(data.generated_at)} 扫描`;
        populateModelFilter();
        renderAll();
        if (selectTop && !state.selectedUid && state.sessions.length) {
          const first = filteredSessions()[0];
          if (first) await showDetails(first.uid);
        } else if (state.selectedUid) {
          await showDetails(state.selectedUid, false);
        }
      } catch (err) {
        document.getElementById('sessionRows').innerHTML = `<tr><td colspan="8" class="error">加载失败：${escapeHtml(err.message)}</td></tr>`;
      } finally {
        refreshBtn.disabled = false;
        refreshBtn.textContent = '刷新';
      }
    }

    function populateModelFilter() {
      const select = document.getElementById('modelFilter');
      const oldValue = select.value;
      const models = Array.from(new Set(state.sessions.map(row => row.model || 'unknown'))).sort();
      select.innerHTML = '<option value="all">全部模型</option>' + models.map(model => `<option value="${escapeHtml(model)}">${escapeHtml(model)}</option>`).join('');
      select.value = models.includes(oldValue) ? oldValue : 'all';
      state.model = select.value;
    }

    function filteredSessions() {
      const needle = state.search.trim().toLowerCase();
      let rows = state.sessions.filter(row => {
        if (state.source !== 'all' && row.source !== state.source) return false;
        if (state.model !== 'all' && (row.model || 'unknown') !== state.model) return false;
        if (!needle) return true;
        const haystack = [
          row.title, row.session_id, row.model, row.project, row.cwd, row.path, row.source
        ].join(' ').toLowerCase();
        return haystack.includes(needle);
      });

      rows.sort((a, b) => {
        let av;
        let bv;
        if (tokenKeys.includes(state.sortKey)) {
          av = tokenValue(a, state.sortKey);
          bv = tokenValue(b, state.sortKey);
        } else if (state.sortKey === 'estimated_cost_usd') {
          av = Number(a.estimated_cost_usd ?? -1);
          bv = Number(b.estimated_cost_usd ?? -1);
        } else if (state.sortKey === 'cached_input_percent' || state.sortKey === 'turn_count') {
          av = Number(a[state.sortKey] ?? -1);
          bv = Number(b[state.sortKey] ?? -1);
        } else if (state.sortKey === 'rank') {
          av = state.sessions.indexOf(a);
          bv = state.sessions.indexOf(b);
        } else {
          av = a[state.sortKey] || '';
          bv = b[state.sortKey] || '';
        }

        if (typeof av === 'number' && typeof bv === 'number') {
          return state.sortDir === 'asc' ? av - bv : bv - av;
        }
        const result = String(av).localeCompare(String(bv), 'zh-CN');
        return state.sortDir === 'asc' ? result : -result;
      });

      if (state.limit !== 'all') rows = rows.slice(0, Number(state.limit));
      return rows;
    }

    function renderAll() {
      renderMetrics();
      updateSortButtons();
      renderTable();
    }

    function setPrimarySort(key) {
      state.sortKey = key;
      state.sortDir = 'desc';
      renderAll();
    }

    function updateSortButtons() {
      document.querySelectorAll('[data-sort-button]').forEach(button => {
        button.classList.toggle('active', button.dataset.sortButton === state.sortKey);
      });
    }

    function renderMetrics() {
      const usage = state.summary && state.summary.usage ? state.summary.usage : {};
      const metrics = [
        ['会话数', fmt(state.summary?.session_count || 0), `当前 ${fmt(state.summary?.active_count || 0)} · 归档 ${fmt(state.summary?.archived_count || 0)}`],
        ['总 tokens', fmtCompact(usage.total_tokens), fmt(usage.total_tokens)],
        ['估算价格', fmtUsd(state.summary?.estimated_cost_usd), `${fmt(state.summary?.estimated_cost_known_count || 0)} 个会话可估算`],
        ['输入 tokens', fmtCompact(usage.input_tokens), fmt(usage.input_tokens)],
        ['缓存输入', fmtCompact(usage.cached_input_tokens), fmt(usage.cached_input_tokens)],
        ['输出 tokens', fmtCompact(usage.output_tokens), fmt(usage.output_tokens)],
      ];
      document.getElementById('metrics').innerHTML = metrics.map(([label, value, hint]) => `
        <div class="metric">
          <div class="label">${escapeHtml(label)}</div>
          <div class="value">${escapeHtml(value)}</div>
          <div class="hint">${escapeHtml(hint)}</div>
        </div>
      `).join('');
    }

    function renderTable() {
      const rows = filteredSessions();
      document.getElementById('resultCount').textContent = `${fmt(rows.length)} 条`;
      if (!rows.length) {
        document.getElementById('sessionRows').innerHTML = '<tr><td colspan="8" class="empty">没有匹配的会话</td></tr>';
        return;
      }
      document.getElementById('sessionRows').innerHTML = rows.map((row, idx) => {
        const usage = usageOf(row);
        const selected = row.uid === state.selectedUid ? 'selected' : '';
        const timeValue = row.end_at || row.updated_at || row.start_at;
        return `
          <tr class="${selected}" data-uid="${escapeHtml(row.uid)}">
            <td class="title-cell" title="${escapeHtml((row.title || row.session_id) + (timeValue ? ' · ' + fmtDate(timeValue) : ''))}">
              <div class="title-line">
                <div class="title-main">${escapeHtml(row.title || row.session_id)}</div>
                <div class="title-time">${escapeHtml(fmtRelativeTime(timeValue))}</div>
              </div>
              <div class="title-sub">${sourceBadge(row.source)} <span class="title-sub-text">${escapeHtml(row.project || shortPath(row.cwd))}</span></div>
            </td>
            <td class="number" title="${fmt(usage.total_tokens)} tokens"><strong>${fmtCompact(usage.total_tokens)}</strong></td>
            <td class="number" title="${fmt(usage.output_tokens)} output tokens">${fmtCompact(usage.output_tokens)}</td>
            <td class="number" title="${row.price_model_known ? '按公开 API 标准价格估算花费' : '没有匹配到公开模型价格'}">${fmtUsd(row.estimated_cost_usd)}</td>
            <td class="number">${fmtPercent(row.cached_input_percent)}</td>
            <td class="number">${fmt(row.turn_count)}</td>
            <td class="model-cell" title="${escapeHtml(row.model || 'unknown')}">${escapeHtml(row.model || 'unknown')}</td>
            <td class="effort-cell" title="${escapeHtml(row.effort || 'N/A')}">${escapeHtml(row.effort || 'N/A')}</td>
          </tr>
        `;
      }).join('');
      document.querySelectorAll('#sessionRows tr[data-uid]').forEach(row => {
        row.addEventListener('click', () => showDetails(row.dataset.uid));
      });
    }

    async function showDetails(uid, renderRows = true) {
      state.selectedUid = uid;
      if (renderRows) renderTable();
      document.getElementById('detailStatus').textContent = '加载中';
      try {
        const res = await fetch('/api/session?id=' + encodeURIComponent(uid), { cache: 'no-store' });
        if (!res.ok) throw new Error('HTTP ' + res.status);
        const detail = await res.json();
        renderDetails(detail);
      } catch (err) {
        document.getElementById('detailsBody').innerHTML = `<div class="error">明细加载失败：${escapeHtml(err.message)}</div>`;
        document.getElementById('detailStatus').textContent = '失败';
      }
    }

    function renderDetails(detail) {
      const usage = usageOf(detail);
      const uncachedInputTokens = Math.max(0, Number(usage.input_tokens || 0) - Number(usage.cached_input_tokens || 0));
      const costs = detail.estimated_cost_breakdown_usd || {};
      const max = Math.max(1, uncachedInputTokens, usage.cached_input_tokens || 0, usage.output_tokens || 0, usage.reasoning_output_tokens || 0);
      const toolRows = Object.entries(detail.tool_counts || {}).slice(0, 8).map(([name, count]) =>
        `<tr><td>${escapeHtml(name)}</td><td class="number">${fmt(count)}</td></tr>`
      ).join('');
      const timelineRows = [...(detail.timeline || [])].sort((a, b) => {
        return new Date(b.timestamp || 0).getTime() - new Date(a.timestamp || 0).getTime();
      }).map((row) => {
        const last = row.last_token_usage || {};
        return `
          <tr>
            <td title="${escapeHtml(fmtDate(row.timestamp))}">${escapeHtml(fmtRelativeTime(row.timestamp))}</td>
            <td class="number">${fmt(last.total_tokens)}</td>
            <td class="number">${fmt(last.input_tokens)}</td>
            <td class="number">${fmt(last.cached_input_tokens)}</td>
            <td class="number">${fmt(last.output_tokens)}</td>
            <td class="number">${fmt(last.reasoning_output_tokens)}</td>
          </tr>
        `;
      }).join('');

      document.getElementById('detailStatus').textContent = `${fmt(detail.token_event_count)} 次计数`;
      document.getElementById('detailsBody').innerHTML = `
        <div class="detail-title">${escapeHtml(detail.title || detail.session_id)}</div>
        <div>${sourceBadge(detail.source)} <span class="badge">${escapeHtml(detail.model || 'unknown')}</span> <span class="badge">${fmt(detail.turn_count)} 轮</span></div>

        <div class="breakdown">
          ${breakdownRow('输入', 'input', uncachedInputTokens, max, costs.input_tokens)}
          ${breakdownRow('缓存', 'cached', usage.cached_input_tokens, max, costs.cached_input_tokens)}
          ${breakdownRow('输出', 'output', usage.output_tokens, max, costs.output_tokens)}
          ${breakdownRow('推理', 'reasoning', usage.reasoning_output_tokens, max, costs.reasoning_output_tokens, '推理花费按输出单价估算，已包含在输出花费中')}
        </div>

        <div class="section-label">累计曲线</div>
        <canvas id="timelineCanvas" width="720" height="220"></canvas>

        <div class="section-label">元数据</div>
        <div class="detail-meta">
          ${kv('总 tokens', fmt(usage.total_tokens))}
          ${kv('花费', fmtUsd(detail.estimated_cost_usd))}
          ${kv('缓存占比', detail.cached_input_percent == null ? '' : detail.cached_input_percent + '%')}
          ${kv('推理强度', detail.effort || 'N/A')}
          ${kv('时间', `${fmtRelativeTime(detail.end_at)} (${fmtDate(detail.start_at)} - ${fmtDate(detail.end_at)})`)}
          ${kv('总耗时', fmtDuration(detail.duration_ms_total))}
          ${kv('TTFT 均值', fmtDuration(detail.time_to_first_token_ms_avg))}
          ${kv('项目', detail.project || '')}
          ${kv('工作目录', detail.cwd || '', 'path')}
          ${kv('日志文件', detail.path || '', 'path')}
          ${kv('Session ID', detail.session_id || '', 'path')}
        </div>

        ${detail.first_user_prompt ? `<div class="section-label">首条用户消息</div><div class="notice">${escapeHtml(detail.first_user_prompt)}</div>` : ''}
        ${detail.last_agent_preview ? `<div class="section-label">最后回复摘要</div><div class="notice">${escapeHtml(detail.last_agent_preview)}</div>` : ''}

        <div class="section-label">工具调用</div>
        ${toolRows ? `<table class="mini-table"><thead><tr><th>工具</th><th>次数</th></tr></thead><tbody>${toolRows}</tbody></table>` : '<div class="notice">没有记录到工具调用。</div>'}

        <div class="section-label">每次计数明细</div>
        ${timelineRows ? `<div class="table-wrap" style="max-height:260px"><table class="mini-table"><thead><tr><th>时间</th><th>本次总量</th><th>输入</th><th>缓存</th><th>输出</th><th>推理</th></tr></thead><tbody>${timelineRows}</tbody></table></div>` : '<div class="notice">这个会话没有 token_count.info 记录。</div>'}
      `;
      drawTimeline(detail.timeline || []);
    }

    function breakdownRow(label, cls, value, max, cost, title = '') {
      const pct = Math.max(2, Math.round(Number(value || 0) / max * 100));
      return `
        <div class="breakdown-row" title="${escapeHtml(title)}">
          <div>${escapeHtml(label)}</div>
          <div class="bar-track"><div class="bar-fill ${cls}" style="width:${pct}%"></div></div>
          <div class="breakdown-value"><span>${fmt(value)}</span><strong>${fmtUsd(cost)}</strong></div>
        </div>
      `;
    }

    function kv(label, value, cls = '') {
      return `<div class="kv"><span>${escapeHtml(label)}</span><strong class="${cls}">${escapeHtml(value || '')}</strong></div>`;
    }

    function drawTimeline(timeline) {
      const canvas = document.getElementById('timelineCanvas');
      if (!canvas) return;
      const rect = canvas.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.max(320, Math.floor(rect.width * dpr));
      canvas.height = Math.floor(150 * dpr);
      const ctx = canvas.getContext('2d');
      ctx.scale(dpr, dpr);
      const width = canvas.width / dpr;
      const height = canvas.height / dpr;
      ctx.clearRect(0, 0, width, height);
      ctx.fillStyle = '#fbfcfc';
      ctx.fillRect(0, 0, width, height);
      ctx.strokeStyle = '#d9dfdd';
      ctx.lineWidth = 1;
      for (let i = 1; i <= 3; i++) {
        const y = Math.round((height / 4) * i);
        ctx.beginPath();
        ctx.moveTo(10, y);
        ctx.lineTo(width - 10, y);
        ctx.stroke();
      }

      const points = timeline.map(row => Number(row.total_token_usage?.total_tokens || 0));
      if (!points.length) {
        ctx.fillStyle = '#65716c';
        ctx.fillText('没有曲线数据', 14, 24);
        return;
      }
      const max = Math.max(1, ...points);
      const min = Math.min(0, ...points);
      const span = Math.max(1, max - min);
      const left = 12;
      const right = width - 12;
      const top = 12;
      const bottom = height - 22;
      ctx.strokeStyle = '#0f7b63';
      ctx.lineWidth = 2;
      ctx.beginPath();
      points.forEach((value, index) => {
        const x = points.length === 1 ? left : left + (right - left) * index / (points.length - 1);
        const y = bottom - (bottom - top) * (value - min) / span;
        if (index === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      });
      ctx.stroke();
      ctx.fillStyle = '#17201d';
      ctx.font = '12px system-ui, sans-serif';
      ctx.fillText(fmtCompact(max), 14, 18);
      ctx.fillStyle = '#65716c';
      ctx.fillText(`${points.length} 次计数`, 14, height - 8);
    }

    document.getElementById('refreshBtn').addEventListener('click', () => loadData(false));
    document.getElementById('exportBtn').addEventListener('click', () => { window.location.href = '/api/export.csv'; });
    document.getElementById('searchInput').addEventListener('input', event => { state.search = event.target.value; renderAll(); });
    document.getElementById('sourceFilter').addEventListener('change', event => { state.source = event.target.value; renderAll(); });
    document.getElementById('modelFilter').addEventListener('change', event => { state.model = event.target.value; renderAll(); });
    document.querySelectorAll('[data-sort-button]').forEach(button => {
      button.addEventListener('click', () => setPrimarySort(button.dataset.sortButton));
    });
    document.getElementById('limitSelect').addEventListener('change', event => { state.limit = event.target.value; renderAll(); });
    document.querySelectorAll('th[data-sort]').forEach(th => {
      th.addEventListener('click', () => {
        const key = th.dataset.sort;
        if (state.sortKey === key) state.sortDir = state.sortDir === 'desc' ? 'asc' : 'desc';
        else { state.sortKey = key; state.sortDir = 'desc'; }
        renderAll();
      });
    });
    window.addEventListener('resize', () => {
      if (state.selectedUid) showDetails(state.selectedUid, false);
    });

    loadData(true);
  </script>
</body>
</html>
"""


def make_handler(analyzer: CodexUsageAnalyzer) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        server_version = "CodexUsageDashboard/1.0"

        def log_message(self, fmt: str, *args: Any) -> None:
            try:
                if sys.stderr is not None and not sys.stderr.closed:
                    sys.stderr.write("%s - %s\n" % (self.address_string(), fmt % args))
            except Exception:
                pass

        def do_GET(self) -> None:
            parsed = urlparse(self.path)
            path = parsed.path
            if path == "/":
                self.send_bytes(HTML.encode("utf-8"), "text/html; charset=utf-8")
                return

            if path == "/api/sessions":
                snapshot = analyzer.scan()
                payload = {
                    "generated_at": snapshot["generated_at"],
                    "codex_home": snapshot["codex_home"],
                    "summary": snapshot["summary"],
                    "sessions": snapshot["sessions"],
                }
                self.send_json(payload)
                return

            if path == "/api/session":
                uid = parse_qs(parsed.query).get("id", [""])[0]
                detail = analyzer.get_detail(uid)
                if detail is None:
                    self.send_json({"error": "session not found"}, status=404)
                    return
                self.send_json(detail)
                return

            if path == "/api/export.csv":
                body = analyzer.export_csv().encode("utf-8-sig")
                self.send_bytes(
                    body,
                    "text/csv; charset=utf-8",
                    extra_headers={"Content-Disposition": 'attachment; filename="codex-usage.csv"'},
                )
                return

            self.send_json({"error": "not found"}, status=404)

        def send_json(self, payload: Any, status: int = 200) -> None:
            body = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
            self.send_bytes(body, "application/json; charset=utf-8", status=status)

        def send_bytes(
            self,
            body: bytes,
            content_type: str,
            status: int = 200,
            extra_headers: dict[str, str] | None = None,
        ) -> None:
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("X-Content-Type-Options", "nosniff")
            if extra_headers:
                for key, value in extra_headers.items():
                    self.send_header(key, value)
            self.end_headers()
            self.wfile.write(body)

    return Handler


def find_free_port(host: str, preferred: int) -> int:
    for port in range(preferred, preferred + 50):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.settimeout(0.2)
            if sock.connect_ex((host, port)) != 0:
                return port
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind((host, 0))
        return int(sock.getsockname()[1])


def existing_dashboard_url(host: str, port: int) -> str | None:
    url = f"http://{host}:{port}/"
    try:
        with urlopen(url + "api/sessions", timeout=0.6) as response:
            if response.status != 200:
                return None
            payload = json.loads(response.read().decode("utf-8"))
    except Exception:
        return None
    if isinstance(payload, dict) and isinstance(payload.get("summary"), dict):
        return url
    return None


def parse_args() -> argparse.Namespace:
    default_home = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex"))
    parser = argparse.ArgumentParser(description="Local read-only Codex usage dashboard.")
    parser.add_argument("--codex-home", type=Path, default=default_home, help="Path to the Codex home directory.")
    parser.add_argument("--host", default="127.0.0.1", help="Bind host. Defaults to 127.0.0.1.")
    parser.add_argument("--port", type=int, default=8765, help="Preferred port. The app will try nearby ports if busy.")
    parser.add_argument("--open", action="store_true", help="Open the dashboard in the default browser.")
    parser.add_argument("--once", action="store_true", help="Scan once and print a short summary instead of serving the UI.")
    parser.add_argument("--json", action="store_true", help="With --once, print JSON.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    analyzer = CodexUsageAnalyzer(args.codex_home)
    if args.once:
        snapshot = analyzer.scan()
        if args.json:
            payload = {
                "generated_at": snapshot["generated_at"],
                "codex_home": snapshot["codex_home"],
                "summary": snapshot["summary"],
                "sessions": snapshot["sessions"],
            }
            safe_print(json.dumps(payload, ensure_ascii=False, indent=2))
        else:
            usage = snapshot["summary"]["usage"]
            safe_print(f"Codex home: {snapshot['codex_home']}")
            safe_print(f"Sessions: {snapshot['summary']['session_count']}")
            safe_print(f"Total tokens: {usage['total_tokens']:,}")
            if snapshot["sessions"]:
                top = snapshot["sessions"][0]
                safe_print(f"Top session: {top.get('title', top.get('session_id'))} ({top['total_token_usage']['total_tokens']:,})")
        return 0

    existing = existing_dashboard_url(args.host, args.port)
    if existing:
        if args.open:
            webbrowser.open(existing, new=2)
        else:
            safe_print(f"Codex Usage Dashboard already running: {existing}")
        return 0

    port = find_free_port(args.host, args.port)
    server = ThreadingHTTPServer((args.host, port), make_handler(analyzer))
    url = f"http://{args.host}:{port}/"
    safe_print(f"Codex Usage Dashboard: {url}")
    safe_print(f"Codex home: {analyzer.codex_home}")
    safe_print("Press Ctrl+C to stop.")
    if args.open:
        threading.Timer(0.4, lambda: webbrowser.open(url, new=2)).start()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        safe_print("\nStopping.")
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
