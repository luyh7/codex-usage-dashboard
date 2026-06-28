#!/usr/bin/env python3
"""Local read-only dashboard for Codex session token usage.

The server reads Codex JSONL logs from:
  - ~/.codex/sessions
  - ~/.codex/archived_sessions
  - Windows ~/.codex when running under WSL and the directory is mounted

It does not modify Codex files. Bind address defaults to 127.0.0.1.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import platform
import re
import socket
import signal
import shutil
import subprocess
import sys
import threading
import time
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, NamedTuple
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

PERIOD_KEYS = {"today", "7d", "30d", "week", "month", "all"}
APP_NAME = "cousash"
SNAPSHOT_SCHEMA = "cousash.remote-snapshot"
SNAPSHOT_VERSION = 1
DASHBOARD_FEATURES = [
    "periods",
    "period-deltas",
    "period-token-label",
    "all-period",
    "calendar-range-v2",
    "multi-codex-home",
    "wsl-windows-autodiscovery",
    "windows-cwd-folder-name",
    "project-grouped-default-view",
    "project-compact-layout-v2",
    "project-env-tag-in-conversation-column",
    "git-worktree-project-grouping",
    "remote-snapshot-import-v1",
]

SUMMARY_KEYS = (
    "uid",
    "session_id",
    "title",
    "source",
    "environment",
    "environment_id",
    "is_remote",
    "remote_device_short_code",
    "remote_imported_at",
    "remote_exported_at",
    "codex_home",
    "path",
    "file_size",
    "parse_errors",
    "created_at",
    "start_at",
    "end_at",
    "updated_at",
    "cwd",
    "project",
    "project_root",
    "workspace_root",
    "project_branch",
    "is_git_worktree",
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
)


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


def subtract_usage(left: dict[str, int], right: dict[str, int]) -> dict[str, int]:
    return {key: max(int(left.get(key, 0)) - int(right.get(key, 0)), 0) for key in TOKEN_KEYS}


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


def utc_iso(moment: dt.datetime) -> str:
    return moment.astimezone(dt.UTC).isoformat().replace("+00:00", "Z")


def parse_timestamp(value: Any) -> dt.datetime | None:
    if not isinstance(value, str) or not value.strip():
        return None
    text = value.strip()
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        parsed = dt.datetime.fromisoformat(text)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=dt.UTC)
    return parsed.astimezone(dt.UTC)


def parse_local_date(value: str | None) -> dt.date | None:
    if not value:
        return None
    try:
        return dt.date.fromisoformat(value)
    except ValueError:
        return None


def local_range_bounds(start_date: str | None, end_date: str | None) -> tuple[dt.datetime, dt.datetime, str, str]:
    now_local = dt.datetime.now().astimezone()
    tzinfo = now_local.tzinfo
    start_day = parse_local_date(start_date) or now_local.date()
    end_day = parse_local_date(end_date) or start_day
    if end_day < start_day:
        start_day, end_day = end_day, start_day

    start_local = dt.datetime.combine(start_day, dt.time.min, tzinfo=tzinfo)
    if end_day >= now_local.date():
        end_local = now_local
    else:
        next_day = end_day + dt.timedelta(days=1)
        end_local = dt.datetime.combine(next_day, dt.time.min, tzinfo=tzinfo) - dt.timedelta(microseconds=1)
    return start_local.astimezone(dt.UTC), end_local.astimezone(dt.UTC), start_day.isoformat(), end_day.isoformat()


def local_period_bounds(
    period: str | None,
    start_date: str | None = None,
    end_date: str | None = None,
) -> tuple[str, dt.datetime | None, dt.datetime, str | None, str | None]:
    key = period if period in PERIOD_KEYS or period == "custom" else "today"
    now_local = dt.datetime.now().astimezone()
    midnight = now_local.replace(hour=0, minute=0, second=0, microsecond=0)

    if key == "all":
        return key, None, now_local.astimezone(dt.UTC), None, None
    if key == "custom":
        start_at, end_at, start_key, end_key = local_range_bounds(start_date, end_date)
        return key, start_at, end_at, start_key, end_key
    if key == "7d":
        start_local = midnight - dt.timedelta(days=6)
    elif key == "30d":
        start_local = midnight - dt.timedelta(days=29)
    elif key == "week":
        start_local = midnight - dt.timedelta(days=midnight.weekday())
    elif key == "month":
        start_local = midnight.replace(day=1)
    else:
        start_local = midnight

    start_key = start_local.date().isoformat()
    end_key = now_local.date().isoformat()
    return key, start_local.astimezone(dt.UTC), now_local.astimezone(dt.UTC), start_key, end_key


def timestamp_in_range(value: Any, start_at: dt.datetime, end_at: dt.datetime) -> bool:
    timestamp = parse_timestamp(value)
    return timestamp is not None and start_at <= timestamp <= end_at


def clean_text(value: str, limit: int = 260) -> str:
    text = re.sub(r"\s+", " ", value).strip()
    if len(text) > limit:
        return text[: limit - 1].rstrip() + "…"
    return text


def folder_name_from_path(value: str) -> str:
    text = str(value or "").strip().strip('"').rstrip("\\/")
    if not text:
        return ""
    parts = [part for part in re.split(r"[\\/]+", text) if part]
    return parts[-1] if parts else text


class ProjectInfo(NamedTuple):
    project: str
    project_root: str
    workspace_root: str
    project_branch: str
    is_git_worktree: bool


def _path_from_cwd(value: str) -> Path | None:
    text = str(value or "").strip().strip('"')
    if not text:
        return None
    if re.match(r"^[A-Za-z]:[\\/]", text):
        if platform.system() == "Windows":
            path = Path(text).expanduser()
            return path if path.exists() else None
        converted = windows_path_to_wsl_path(text)
        return converted if converted is not None and converted.exists() else None
    path = Path(text).expanduser()
    return path if path.exists() else None


def _git_output(cwd: Path, *args: str) -> str:
    git = shutil.which("git")
    if not git:
        return ""
    return run_text_quiet([git, "-C", str(cwd), *args])


def _git_common_dir(workspace: Path) -> Path | None:
    output = _git_output(workspace, "rev-parse", "--path-format=absolute", "--git-common-dir")
    if not output:
        return None
    try:
        return Path(output.splitlines()[-1]).expanduser().resolve()
    except OSError:
        return None


def _git_workspace_root(cwd: Path) -> Path | None:
    output = _git_output(cwd, "rev-parse", "--show-toplevel")
    if not output:
        return None
    try:
        return Path(output.splitlines()[-1]).expanduser().resolve()
    except OSError:
        return None


def _parse_git_worktree_list(output: str) -> tuple[Path | None, dict[str, str]]:
    main_workspace: Path | None = None
    branches: dict[str, str] = {}
    current_workspace: Path | None = None
    current_bare = False
    current_branch = ""

    def finish_entry() -> None:
        nonlocal main_workspace, current_workspace, current_bare, current_branch
        if current_workspace is None:
            return
        workspace_key = str(current_workspace)
        branches[workspace_key] = current_branch
        if main_workspace is None and not current_bare:
            main_workspace = current_workspace

    for line in output.splitlines():
        if not line.strip():
            finish_entry()
            current_workspace = None
            current_bare = False
            current_branch = ""
            continue
        if line.startswith("worktree "):
            finish_entry()
            current_bare = False
            current_branch = ""
            raw_path = line.removeprefix("worktree ").strip()
            try:
                current_workspace = Path(raw_path).expanduser().resolve()
            except OSError:
                current_workspace = None
        elif line == "bare":
            current_bare = True
        elif line.startswith("branch "):
            current_branch = line.removeprefix("branch ").strip().removeprefix("refs/heads/")

    finish_entry()
    return main_workspace, branches


def git_project_info(cwd: str) -> ProjectInfo:
    fallback_project = folder_name_from_path(cwd)
    fallback_root = str(cwd or "").strip()
    path = _path_from_cwd(cwd)
    if path is None:
        return ProjectInfo(fallback_project, fallback_root, fallback_root, "", False)

    workspace = _git_workspace_root(path)
    if workspace is None:
        root = str(path.resolve()) if path.exists() else fallback_root
        return ProjectInfo(folder_name_from_path(root), root, root, "", False)

    branch = _git_output(workspace, "branch", "--show-current")
    branch = branch.splitlines()[-1].strip() if branch else ""
    project_root = workspace
    is_worktree = False
    common_dir = _git_common_dir(workspace)
    if common_dir is not None:
        output = _git_output(workspace, "worktree", "list", "--porcelain")
        main_workspace, branches = _parse_git_worktree_list(output)
        workspace_key = str(workspace)
        if branches.get(workspace_key):
            branch = branches[workspace_key]
        if main_workspace is not None:
            project_root = main_workspace
            is_worktree = workspace != main_workspace

    project_root_text = str(project_root)
    return ProjectInfo(
        folder_name_from_path(project_root_text),
        project_root_text,
        str(workspace),
        branch,
        is_worktree,
    )


def safe_print(*values: Any) -> None:
    try:
        if sys.stdout is not None and not sys.stdout.closed:
            print(*values)
    except Exception:
        pass


def app_config_dir() -> Path:
    override = os.environ.get("COUSASH_CONFIG_DIR")
    if override:
        return Path(override).expanduser()
    system = platform.system()
    if system == "Darwin":
        return Path.home() / "Library" / "Application Support" / APP_NAME
    if system == "Windows":
        root = os.environ.get("APPDATA")
        return Path(root) / APP_NAME if root else Path.home() / "AppData" / "Roaming" / APP_NAME
    root = os.environ.get("XDG_CONFIG_HOME")
    return Path(root) / APP_NAME if root else Path.home() / ".config" / APP_NAME


def remote_snapshots_dir() -> Path:
    return app_config_dir() / "remotes"


def safe_json_dump(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True), encoding="utf-8")
    tmp.replace(path)


def read_json_file(path: Path) -> dict[str, Any] | None:
    try:
        payload = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError):
        return None
    return payload if isinstance(payload, dict) else None


def run_text_quiet(command: list[str]) -> str:
    try:
        completed = subprocess.run(
            command,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            timeout=2.0,
        )
    except (OSError, subprocess.SubprocessError):
        return ""
    return completed.stdout.strip() if completed.returncode == 0 else ""


def mac_platform_uuid() -> str:
    output = run_text_quiet(["ioreg", "-rd1", "-c", "IOPlatformExpertDevice"])
    match = re.search(r'"IOPlatformUUID"\s*=\s*"([^"]+)"', output)
    return match.group(1).strip() if match else ""


def windows_machine_guid() -> str:
    output = run_text_quiet(
        [
            "reg",
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ]
    )
    match = re.search(r"MachineGuid\s+REG_\w+\s+([^\r\n]+)", output)
    if match:
        return match.group(1).strip()
    output = run_text_quiet(["powershell", "-NoProfile", "-Command", "(Get-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Cryptography').MachineGuid"])
    return output.splitlines()[-1].strip() if output else ""


def linux_machine_id() -> str:
    for path in (Path("/etc/machine-id"), Path("/var/lib/dbus/machine-id")):
        try:
            value = path.read_text(encoding="utf-8").strip()
        except OSError:
            continue
        if value:
            return value
    return ""


def device_code_prefix() -> str:
    system = platform.system()
    if system == "Darwin":
        return "mac"
    if system == "Windows":
        return "win"
    if running_in_wsl():
        return "wsl"
    if system == "Linux":
        return "linux"
    return slugify_source_id(system or "device")


def stable_device_identity() -> str:
    system = platform.system()
    if system == "Darwin":
        value = mac_platform_uuid()
    elif system == "Windows":
        value = windows_machine_guid()
    else:
        value = linux_machine_id()
    return value.strip()


def fallback_device_seed() -> str:
    path = app_config_dir() / "device-seed.json"
    payload = read_json_file(path)
    if payload and isinstance(payload.get("seed"), str) and payload["seed"]:
        return payload["seed"]
    seed = hashlib.sha256(f"{time.time_ns()}:{os.urandom(16).hex()}".encode("utf-8")).hexdigest()
    safe_json_dump(path, {"seed": seed, "created_at": utc_iso(dt.datetime.now(dt.UTC))})
    return seed


def current_device_short_code() -> str:
    prefix = device_code_prefix()
    identity = stable_device_identity()
    if not identity:
        identity = fallback_device_seed()
    digest = hashlib.sha256(f"{prefix}:{identity}".encode("utf-8", errors="replace")).hexdigest()[:8]
    return f"{prefix}-{digest}"


def default_device_label() -> str:
    node = platform.node().strip()
    system = platform.system() or "Device"
    if node:
        return node
    if system == "Darwin":
        return "Mac"
    if system == "Windows":
        return "Windows PC"
    if running_in_wsl():
        return "WSL"
    return system


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


def is_synthetic_user_context(text: str) -> bool:
    stripped = text.strip()
    return stripped.startswith("# AGENTS.md instructions for ") or stripped.startswith("<environment_context>")


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


class CodexLogSource(NamedTuple):
    id: str
    label: str
    codex_home: Path


def slugify_source_id(value: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
    return slug or "codex"


def path_has_codex_logs(path: Path) -> bool:
    home = path.expanduser()
    return (
        (home / "sessions").exists()
        or (home / "archived_sessions").exists()
        or (home / "session_index.jsonl").exists()
    )


def running_in_wsl() -> bool:
    if os.environ.get("WSL_DISTRO_NAME") or os.environ.get("WSL_INTEROP"):
        return True
    try:
        release = Path("/proc/sys/kernel/osrelease").read_text(encoding="utf-8", errors="replace").lower()
    except OSError:
        release = platform.uname().release.lower()
    return "microsoft" in release or "wsl" in release


def windows_path_to_wsl_path(value: str) -> Path | None:
    text = value.strip().strip('"').replace("\r", "")
    match = re.match(r"^([A-Za-z]):[\\/](.*)$", text)
    if not match:
        return None
    drive = match.group(1).lower()
    rest = match.group(2).replace("\\", "/").strip("/")
    return Path("/mnt") / drive / rest


def windows_codex_home_candidates() -> list[Path]:
    candidates: list[Path] = []
    userprofile = run_text(["cmd.exe", "/c", "echo", "%USERPROFILE%"])
    if userprofile:
        profile = windows_path_to_wsl_path(userprofile.splitlines()[-1])
        if profile is not None:
            candidates.append(profile / ".codex")

    for username in (os.environ.get("USER"), os.environ.get("USERNAME")):
        if username:
            candidates.append(Path("/mnt/c/Users") / username / ".codex")

    if not any(path_has_codex_logs(path) for path in candidates):
        users_dir = Path("/mnt/c/Users")
        try:
            matches = [path for path in users_dir.glob("*/.codex") if path_has_codex_logs(path)]
        except OSError:
            matches = []
        if len(matches) == 1:
            candidates.extend(matches)

    seen: set[str] = set()
    deduped: list[Path] = []
    for path in candidates:
        key = str(path)
        if key in seen:
            continue
        seen.add(key)
        deduped.append(path)
    return deduped


def label_for_codex_home(path: Path) -> str:
    text = str(path)
    if running_in_wsl():
        if text.startswith("/mnt/c/Users/"):
            return "Windows"
        return "WSL"
    system = platform.system()
    if system == "Darwin":
        return "macOS"
    if system:
        return system
    return "Local"


def make_log_source(path: Path, label: str | None = None, source_id: str | None = None) -> CodexLogSource:
    resolved = path.expanduser().resolve()
    source_label = label or label_for_codex_home(resolved)
    return CodexLogSource(source_id or slugify_source_id(source_label), source_label, resolved)


def dedupe_codex_sources(sources: list[CodexLogSource]) -> list[CodexLogSource]:
    seen_paths: set[str] = set()
    seen_ids: set[str] = set()
    deduped: list[CodexLogSource] = []
    for source in sources:
        path_key = str(source.codex_home)
        if path_key in seen_paths:
            continue
        seen_paths.add(path_key)

        source_id = source.id
        if source_id in seen_ids:
            suffix = 2
            while f"{source_id}-{suffix}" in seen_ids:
                suffix += 1
            source_id = f"{source_id}-{suffix}"
        seen_ids.add(source_id)
        deduped.append(CodexLogSource(source_id, source.label, source.codex_home))
    return deduped


def codex_sources_from_homes(homes: list[Path]) -> list[CodexLogSource]:
    return dedupe_codex_sources([make_log_source(path) for path in homes])


def default_codex_sources(include_windows: bool = True) -> list[CodexLogSource]:
    local_home = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex"))
    sources = [make_log_source(local_home)]
    if include_windows and running_in_wsl():
        for path in windows_codex_home_candidates():
            if path_has_codex_logs(path):
                sources.append(make_log_source(path, "Windows", "windows"))
    return dedupe_codex_sources(sources)


def codex_source_payloads(sources: list[CodexLogSource]) -> list[dict[str, Any]]:
    return [
        {"id": source.id, "label": source.label, "codex_home": str(source.codex_home), "is_remote": False}
        for source in sources
    ]


def codex_home_display(sources: list[CodexLogSource]) -> str:
    if len(sources) == 1:
        return str(sources[0].codex_home)
    return " · ".join(f"{source.label}: {source.codex_home}" for source in sources)


def safe_device_code(value: Any) -> str:
    code = str(value or "").strip().lower()
    code = re.sub(r"[^a-z0-9_-]+", "-", code).strip("-")
    return code[:80]


def snapshot_session_key(session: dict[str, Any]) -> str:
    for key in ("session_id", "uid", "path"):
        value = session.get(key)
        if isinstance(value, str) and value:
            return value
    return hashlib.sha1(json.dumps(session, sort_keys=True, ensure_ascii=False).encode("utf-8")).hexdigest()


def clone_json(value: Any) -> Any:
    return json.loads(json.dumps(value, ensure_ascii=False))


def path_state_signature(path: Path) -> tuple[str, int | None, int | None]:
    try:
        stat = path.stat()
    except OSError:
        return (str(path), None, None)
    return (str(path), stat.st_mtime_ns, stat.st_size)


class RemoteSnapshotStore:
    def __init__(self, current_device_code: str | None = None, root: Path | None = None):
        self.current_device_code = safe_device_code(current_device_code or current_device_short_code())
        self.root = root or remote_snapshots_dir()

    def snapshot_path(self, device_code: str) -> Path:
        code = safe_device_code(device_code)
        if not code:
            raise ValueError("missing device short code")
        return self.root / f"{code}.json"

    def validate_snapshot(self, payload: Any) -> tuple[str, dict[str, Any], dict[str, Any]]:
        if not isinstance(payload, dict):
            raise ValueError("snapshot must be a JSON object")
        if payload.get("schema") != SNAPSHOT_SCHEMA:
            raise ValueError("unsupported snapshot schema")
        try:
            version = int(payload.get("version") or 0)
        except (TypeError, ValueError) as exc:
            raise ValueError("snapshot version is invalid") from exc
        if version > SNAPSHOT_VERSION:
            raise ValueError("snapshot version is newer than this dashboard")
        device = payload.get("device")
        snapshot = payload.get("snapshot")
        if not isinstance(device, dict) or not isinstance(snapshot, dict):
            raise ValueError("snapshot is missing device or data")
        code = safe_device_code(device.get("short_code"))
        if not code:
            raise ValueError("snapshot is missing device short code")
        sessions = snapshot.get("sessions")
        details = snapshot.get("details_by_uid")
        if not isinstance(sessions, list) or not isinstance(details, dict):
            raise ValueError("snapshot is missing session data")
        return code, device, snapshot

    def read_remote(self, device_code: str) -> dict[str, Any] | None:
        return read_json_file(self.snapshot_path(device_code))

    def read_all(self) -> list[dict[str, Any]]:
        try:
            paths = sorted(self.root.glob("*.json"))
        except OSError:
            return []
        payloads: list[dict[str, Any]] = []
        for path in paths:
            payload = read_json_file(path)
            if not payload:
                continue
            try:
                code, _device, _snapshot = self.validate_snapshot(payload)
            except ValueError:
                continue
            payloads.append(payload)
        return payloads

    def state_signature(self) -> tuple[tuple[str, int | None, int | None], ...]:
        try:
            paths = sorted(self.root.glob("*.json"))
        except OSError:
            return ()
        return tuple(path_state_signature(path) for path in paths)

    def list_remotes(self) -> list[dict[str, Any]]:
        rows: list[dict[str, Any]] = []
        for payload in self.read_all():
            try:
                code, device, snapshot = self.validate_snapshot(payload)
            except ValueError:
                continue
            sessions = snapshot.get("sessions") if isinstance(snapshot.get("sessions"), list) else []
            usage = normalize_usage((snapshot.get("summary") or {}).get("usage") if isinstance(snapshot.get("summary"), dict) else None)
            rows.append(
                {
                    "device_short_code": code,
                    "label": str(device.get("label") or code),
                    "platform": str(device.get("platform") or ""),
                    "hostname": str(device.get("hostname") or ""),
                    "session_count": len(sessions),
                    "usage": usage,
                    "imported_at": payload.get("imported_at") or "",
                    "exported_at": payload.get("exported_at") or "",
                    "generated_at": snapshot.get("generated_at") or "",
                }
            )
        return sorted(rows, key=lambda row: str(row.get("label") or row.get("device_short_code")))

    def import_snapshot(
        self,
        incoming: dict[str, Any],
        label: str | None = None,
        allow_current_device: bool = False,
    ) -> dict[str, Any]:
        code, incoming_device, incoming_snapshot = self.validate_snapshot(incoming)
        if code == self.current_device_code and not allow_current_device:
            return {
                "ok": False,
                "needs_confirmation": True,
                "reason": "current_device",
                "device_short_code": code,
                "suggested_label": str(incoming_device.get("label") or code),
            }

        existing = self.read_remote(code)
        existing_device: dict[str, Any] = {}
        existing_snapshot: dict[str, Any] = {}
        if existing:
            try:
                _existing_code, existing_device, existing_snapshot = self.validate_snapshot(existing)
            except ValueError:
                existing_device = {}
                existing_snapshot = {}

        new_label = clean_text(label or str(existing_device.get("label") or incoming_device.get("label") or code), 120)
        if not existing and not label:
            return {
                "ok": False,
                "needs_label": True,
                "device_short_code": code,
                "suggested_label": new_label,
                "platform": incoming_device.get("platform") or "",
                "hostname": incoming_device.get("hostname") or "",
            }

        merged_snapshot = self.merge_snapshots(existing_snapshot, incoming_snapshot)
        now = utc_iso(dt.datetime.now(dt.UTC))
        stored = {
            "schema": SNAPSHOT_SCHEMA,
            "version": SNAPSHOT_VERSION,
            "device": {
                **{key: value for key, value in incoming_device.items() if isinstance(key, str)},
                "short_code": code,
                "label": new_label,
            },
            "exported_at": incoming.get("exported_at") or incoming_snapshot.get("generated_at") or now,
            "imported_at": now,
            "snapshot": merged_snapshot,
        }
        safe_json_dump(self.snapshot_path(code), stored)
        return {"ok": True, "remote": self.remote_metadata(stored), "created": not bool(existing)}

    def merge_snapshots(self, existing: dict[str, Any], incoming: dict[str, Any]) -> dict[str, Any]:
        existing_sessions = existing.get("sessions") if isinstance(existing.get("sessions"), list) else []
        incoming_sessions = incoming.get("sessions") if isinstance(incoming.get("sessions"), list) else []
        existing_details = existing.get("details_by_uid") if isinstance(existing.get("details_by_uid"), dict) else {}
        incoming_details = incoming.get("details_by_uid") if isinstance(incoming.get("details_by_uid"), dict) else {}

        by_key: dict[str, dict[str, Any]] = {}
        detail_by_uid: dict[str, dict[str, Any]] = {}
        for session in existing_sessions:
            if isinstance(session, dict):
                cloned = clone_json(session)
                by_key[snapshot_session_key(cloned)] = cloned
                uid = cloned.get("uid")
                if isinstance(uid, str) and isinstance(existing_details.get(uid), dict):
                    detail_by_uid[uid] = clone_json(existing_details[uid])

        for session in incoming_sessions:
            if not isinstance(session, dict):
                continue
            cloned = clone_json(session)
            key = snapshot_session_key(cloned)
            old = by_key.get(key)
            old_uid = old.get("uid") if isinstance(old, dict) else None
            if isinstance(old_uid, str):
                detail_by_uid.pop(old_uid, None)
            by_key[key] = cloned
            uid = cloned.get("uid")
            if isinstance(uid, str) and isinstance(incoming_details.get(uid), dict):
                detail_by_uid[uid] = clone_json(incoming_details[uid])

        sessions = list(by_key.values())
        sessions.sort(key=lambda row: str(row.get("end_at") or row.get("updated_at") or row.get("start_at") or ""), reverse=True)
        generated_at = incoming.get("generated_at") or utc_iso(dt.datetime.now(dt.UTC))
        return {
            "generated_at": generated_at,
            "codex_home": incoming.get("codex_home") or existing.get("codex_home") or "",
            "codex_sources": incoming.get("codex_sources") if isinstance(incoming.get("codex_sources"), list) else [],
            "sessions": sessions,
            "details_by_uid": detail_by_uid,
            "summary": CodexUsageAnalyzer.build_summary_static(sessions),
            "daily_usage": CodexUsageAnalyzer.build_daily_usage_static(detail_by_uid.values()),
        }

    def remote_metadata(self, payload: dict[str, Any]) -> dict[str, Any]:
        code, device, snapshot = self.validate_snapshot(payload)
        sessions = snapshot.get("sessions") if isinstance(snapshot.get("sessions"), list) else []
        summary = snapshot.get("summary") if isinstance(snapshot.get("summary"), dict) else {}
        return {
            "device_short_code": code,
            "label": str(device.get("label") or code),
            "platform": str(device.get("platform") or ""),
            "hostname": str(device.get("hostname") or ""),
            "session_count": len(sessions),
            "usage": normalize_usage(summary.get("usage")),
            "imported_at": payload.get("imported_at") or "",
            "exported_at": payload.get("exported_at") or "",
            "generated_at": snapshot.get("generated_at") or "",
        }

    def rename_remote(self, device_code: str, label: str) -> dict[str, Any]:
        code = safe_device_code(device_code)
        payload = self.read_remote(code)
        if not payload:
            raise FileNotFoundError(code)
        validated_code, device, _snapshot = self.validate_snapshot(payload)
        device["label"] = clean_text(label, 120) or validated_code
        payload["device"] = device
        safe_json_dump(self.snapshot_path(validated_code), payload)
        return self.remote_metadata(payload)

    def delete_remote(self, device_code: str) -> None:
        path = self.snapshot_path(device_code)
        if not path.exists():
            raise FileNotFoundError(device_code)
        path.unlink()

    def transformed_sessions(self) -> tuple[list[dict[str, Any]], dict[str, dict[str, Any]], list[dict[str, str]]]:
        sessions: list[dict[str, Any]] = []
        details: dict[str, dict[str, Any]] = {}
        sources: list[dict[str, str]] = []
        for payload in self.read_all():
            try:
                code, device, snapshot = self.validate_snapshot(payload)
            except ValueError:
                continue
            label = str(device.get("label") or code)
            source_id = f"remote-{code}"
            sources.append({"id": source_id, "label": label, "codex_home": f"remote:{code}", "is_remote": True})
            raw_details = snapshot.get("details_by_uid") if isinstance(snapshot.get("details_by_uid"), dict) else {}
            for session in snapshot.get("sessions", []):
                if not isinstance(session, dict):
                    continue
                transformed = self.transform_row(session, code, label, source_id, payload)
                sessions.append({key: transformed.get(key) for key in SUMMARY_KEYS})
                raw_uid = session.get("uid")
                raw_detail = raw_details.get(raw_uid) if isinstance(raw_uid, str) else None
                if isinstance(raw_detail, dict):
                    details[transformed["uid"]] = self.transform_row(raw_detail, code, label, source_id, payload, transformed["uid"])
        return sessions, details, sources

    def transform_row(
        self,
        row: dict[str, Any],
        code: str,
        label: str,
        source_id: str,
        payload: dict[str, Any],
        forced_uid: str | None = None,
    ) -> dict[str, Any]:
        raw_uid = str(row.get("uid") or row.get("session_id") or row.get("path") or "")
        uid = forced_uid or hashlib.sha1(f"{code}:{raw_uid}".encode("utf-8", errors="replace")).hexdigest()[:16]
        transformed = clone_json(row)
        transformed["uid"] = uid
        transformed["environment"] = label
        transformed["environment_id"] = source_id
        transformed["is_remote"] = True
        transformed["remote_device_short_code"] = code
        transformed["remote_imported_at"] = payload.get("imported_at") or ""
        transformed["remote_exported_at"] = payload.get("exported_at") or ""
        transformed["codex_home"] = f"remote:{code}"
        return transformed


class CodexUsageAnalyzer:
    def __init__(
        self,
        codex_home: Path | list[Path] | list[CodexLogSource],
        remote_store: RemoteSnapshotStore | None = None,
    ):
        if isinstance(codex_home, list):
            if codex_home and isinstance(codex_home[0], CodexLogSource):
                sources = dedupe_codex_sources(codex_home)
            else:
                sources = codex_sources_from_homes([Path(item) for item in codex_home])
        else:
            sources = codex_sources_from_homes([codex_home])
        if not sources:
            sources = codex_sources_from_homes([Path.home() / ".codex"])
        self.codex_sources = sources
        self.codex_home = sources[0].codex_home
        self.codex_home_display = codex_home_display(sources)
        self.remote_store = remote_store
        self._cache: dict[str, tuple[int, int, dict[str, Any], dict[str, Any]]] = {}
        self._snapshot_cache_signature: tuple[Any, ...] | None = None
        self._snapshot_cache: dict[str, Any] | None = None
        self._period_cache: dict[tuple[str, str | None, str | None], dict[str, Any]] = {}
        self._project_info_cache: dict[str, ProjectInfo] = {}

    def load_session_titles(self, codex_home: Path | None = None) -> dict[str, str]:
        titles: dict[str, str] = {}
        index_path = (codex_home or self.codex_home) / "session_index.jsonl"
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

    def iter_session_files(self) -> list[tuple[CodexLogSource, Path, str]]:
        files: list[tuple[CodexLogSource, Path, str]] = []
        for log_source in self.codex_sources:
            sessions_dir = log_source.codex_home / "sessions"
            archived_dir = log_source.codex_home / "archived_sessions"

            if sessions_dir.exists():
                for path in sessions_dir.rglob("*.jsonl"):
                    files.append((log_source, path, "active"))
            if archived_dir.exists():
                for path in archived_dir.glob("*.jsonl"):
                    files.append((log_source, path, "archived"))

        files.sort(key=lambda item: item[1].stat().st_mtime if item[1].exists() else 0, reverse=True)
        return files

    def scan_signature(self, files: list[tuple[CodexLogSource, Path, str]], include_remotes: bool) -> tuple[Any, ...]:
        file_signature = tuple(
            (log_source.id, source, *path_state_signature(path))
            for log_source, path, source in files
        )
        title_signature = tuple(
            (log_source.id, *path_state_signature(log_source.codex_home / "session_index.jsonl"))
            for log_source in self.codex_sources
        )
        remote_signature = self.remote_store.state_signature() if include_remotes and self.remote_store is not None else ()
        return (file_signature, title_signature, remote_signature)

    def scan(
        self,
        period: str | None = None,
        start_date: str | None = None,
        end_date: str | None = None,
        include_remotes: bool = True,
    ) -> dict[str, Any]:
        files = self.iter_session_files()
        signature = self.scan_signature(files, include_remotes)
        if self._snapshot_cache_signature != signature or self._snapshot_cache is None:
            self._snapshot_cache = self.build_snapshot(files, include_remotes)
            self._snapshot_cache_signature = signature
            self._period_cache = {}

        snapshot = self._snapshot_cache
        if period:
            cache_key = (period, start_date, end_date)
            cached = self._period_cache.get(cache_key)
            if cached is None:
                cached = self.filter_snapshot_by_period(snapshot, period, start_date, end_date)
                self._period_cache[cache_key] = cached
            return cached
        return snapshot

    def build_snapshot(
        self,
        files: list[tuple[CodexLogSource, Path, str]],
        include_remotes: bool,
    ) -> dict[str, Any]:
        titles_by_source = {
            log_source.id: self.load_session_titles(log_source.codex_home)
            for log_source in self.codex_sources
        }
        sessions: list[dict[str, Any]] = []
        details_by_uid: dict[str, dict[str, Any]] = {}

        for log_source, path, source in files:
            try:
                summary, detail = self.parse_file_cached(path, source, log_source)
            except OSError:
                continue

            session_id = summary.get("session_id")
            titles = titles_by_source.get(log_source.id, {})
            if isinstance(session_id, str) and session_id in titles:
                summary["title"] = titles[session_id]
                detail["title"] = titles[session_id]

            sessions.append(summary)
            details_by_uid[summary["uid"]] = detail

        codex_sources: list[dict[str, Any]] = codex_source_payloads(self.codex_sources)
        if include_remotes and self.remote_store is not None:
            remote_sessions, remote_details, remote_sources = self.remote_store.transformed_sessions()
            sessions.extend(remote_sessions)
            details_by_uid.update(remote_details)
            codex_sources.extend(remote_sources)

        sessions.sort(key=lambda row: row["total_token_usage"].get("total_tokens", 0), reverse=True)
        generated_at = dt.datetime.now(dt.UTC).isoformat().replace("+00:00", "Z")
        snapshot = {
            "generated_at": generated_at,
            "codex_home": self.codex_home_display,
            "codex_sources": codex_sources,
            "sessions": sessions,
            "details_by_uid": details_by_uid,
            "summary": self.build_summary(sessions),
            "daily_usage": self.build_daily_usage(details_by_uid.values()),
            "period": {"key": "all", "start_at": None, "end_at": generated_at},
        }
        return snapshot

    def filter_snapshot_by_period(
        self,
        snapshot: dict[str, Any],
        period: str,
        start_date: str | None = None,
        end_date: str | None = None,
    ) -> dict[str, Any]:
        key, start_at, end_at, start_key, end_key = local_period_bounds(period, start_date, end_date)
        if key == "all" or start_at is None:
            return {
                **snapshot,
                "period": {"key": "all", "start_at": None, "end_at": utc_iso(end_at), "start_date": None, "end_date": None},
            }

        sessions: list[dict[str, Any]] = []
        details_by_uid: dict[str, dict[str, Any]] = {}

        for session in snapshot["sessions"]:
            detail = snapshot["details_by_uid"].get(session["uid"])
            if not detail:
                continue
            ranged_detail = self.detail_for_period(detail, start_at, end_at)
            usage = normalize_usage(ranged_detail.get("total_token_usage"))
            if ranged_detail.get("token_event_count", 0) <= 0 and usage["total_tokens"] <= 0:
                continue
            sessions.append({field: ranged_detail.get(field) for field in SUMMARY_KEYS})
            details_by_uid[ranged_detail["uid"]] = ranged_detail

        sessions.sort(key=lambda row: row["total_token_usage"].get("total_tokens", 0), reverse=True)
        return {
            **snapshot,
            "sessions": sessions,
            "details_by_uid": details_by_uid,
            "summary": self.build_summary(sessions),
            "period": {
                "key": key,
                "start_at": utc_iso(start_at),
                "end_at": utc_iso(end_at),
                "start_date": start_key,
                "end_date": end_key,
            },
        }

    def build_daily_usage(self, details: Any) -> list[dict[str, Any]]:
        return self.build_daily_usage_static(details)

    @staticmethod
    def build_daily_usage_static(details: Any) -> list[dict[str, Any]]:
        tzinfo = dt.datetime.now().astimezone().tzinfo
        by_day: dict[str, dict[str, Any]] = {}

        for detail in details:
            all_timeline = [
                row
                for row in detail.get("timeline", [])
                if parse_timestamp(row.get("timestamp")) is not None
            ]
            all_timeline.sort(key=lambda row: parse_timestamp(row.get("timestamp")) or dt.datetime.min.replace(tzinfo=dt.UTC))
            previous_usage = zero_usage()
            for row in all_timeline:
                timestamp = parse_timestamp(row.get("timestamp"))
                if timestamp is None:
                    continue
                cumulative_usage = normalize_usage(row.get("total_token_usage"))
                delta_usage = subtract_usage(cumulative_usage, previous_usage)
                previous_usage = cumulative_usage
                if delta_usage["total_tokens"] <= 0:
                    continue
                day = timestamp.astimezone(tzinfo).date().isoformat()
                by_day.setdefault(day, {"date": day, "usage": zero_usage()})
                by_day[day]["usage"] = add_usage(by_day[day]["usage"], delta_usage)

        return sorted(by_day.values(), key=lambda row: row["date"])

    def detail_for_period(self, detail: dict[str, Any], start_at: dt.datetime, end_at: dt.datetime) -> dict[str, Any]:
        all_timeline = [
            row
            for row in detail.get("timeline", [])
            if parse_timestamp(row.get("timestamp")) is not None
        ]
        all_timeline.sort(key=lambda row: parse_timestamp(row.get("timestamp")) or dt.datetime.min.replace(tzinfo=dt.UTC))

        baseline_usage = zero_usage()
        timeline: list[dict[str, Any]] = []
        previous_usage = zero_usage()
        for row in all_timeline:
            timestamp = parse_timestamp(row.get("timestamp"))
            if timestamp is None:
                continue
            cumulative_usage = normalize_usage(row.get("total_token_usage"))
            if timestamp < start_at:
                baseline_usage = cumulative_usage
                previous_usage = cumulative_usage
                continue
            if timestamp > end_at:
                break

            relative_row = dict(row)
            relative_row["total_token_usage"] = subtract_usage(cumulative_usage, baseline_usage)
            relative_row["last_token_usage"] = subtract_usage(cumulative_usage, previous_usage)
            timeline.append(relative_row)
            previous_usage = cumulative_usage

        end_usage = normalize_usage(all_timeline[-1].get("total_token_usage")) if timeline else zero_usage()
        if timeline:
            end_usage = add_usage(baseline_usage, normalize_usage(timeline[-1].get("total_token_usage")))
        total_usage = subtract_usage(end_usage, baseline_usage)
        last_usage = normalize_usage(timeline[-1].get("last_token_usage")) if timeline else zero_usage()
        cached_percent = None
        input_tokens = total_usage.get("input_tokens", 0)
        if input_tokens:
            cached_percent = round(total_usage.get("cached_input_tokens", 0) / input_tokens * 100, 1)

        tasks = [
            row
            for row in detail.get("tasks", [])
            if timestamp_in_range(row.get("timestamp"), start_at, end_at)
        ]
        durations_ms = [
            int(row["duration_ms"])
            for row in tasks
            if isinstance(row.get("duration_ms"), (int, float))
        ]
        ttf_ms = [
            int(row["time_to_first_token_ms"])
            for row in tasks
            if isinstance(row.get("time_to_first_token_ms"), (int, float))
        ]

        ranged = dict(detail)
        ranged["period_start_at"] = utc_iso(start_at)
        ranged["period_end_at"] = utc_iso(end_at)
        ranged["timeline"] = timeline
        ranged["tasks"] = tasks
        ranged["total_token_usage"] = total_usage
        ranged["last_token_usage"] = last_usage
        ranged["estimated_cost_usd"] = estimate_cost_usd(total_usage, str(detail.get("model") or ""))
        ranged["estimated_cost_breakdown_usd"] = estimate_cost_breakdown_usd(total_usage, str(detail.get("model") or ""))
        ranged["price_model_known"] = ranged["estimated_cost_usd"] is not None
        ranged["cached_input_percent"] = cached_percent
        ranged["token_event_count"] = len(timeline)
        ranged["turn_count"] = len(tasks) or len(timeline)
        ranged["completed_turn_count"] = len(tasks)
        ranged["duration_ms_total"] = sum(durations_ms)
        ranged["duration_ms_avg"] = int(sum(durations_ms) / len(durations_ms)) if durations_ms else None
        ranged["time_to_first_token_ms_avg"] = int(sum(ttf_ms) / len(ttf_ms)) if ttf_ms else None
        if timeline:
            ranged["start_at"] = str(timeline[0].get("timestamp") or detail.get("start_at") or "")
            ranged["end_at"] = str(timeline[-1].get("timestamp") or detail.get("end_at") or "")
            ranged["model_context_window"] = timeline[-1].get("model_context_window")
            ranged["latest_rate_limits"] = timeline[-1].get("rate_limits")
        return ranged

    def parse_file_cached(
        self,
        path: Path,
        source: str,
        log_source: CodexLogSource | None = None,
    ) -> tuple[dict[str, Any], dict[str, Any]]:
        log_source = log_source or self.codex_sources[0]
        stat = path.stat()
        cache_key = f"{log_source.id}:{path.resolve()}"
        cached = self._cache.get(cache_key)
        if cached and cached[0] == stat.st_mtime_ns and cached[1] == stat.st_size:
            return cached[2], cached[3]

        summary, detail = self.parse_file(path, source, log_source)
        self._cache[cache_key] = (stat.st_mtime_ns, stat.st_size, summary, detail)
        return summary, detail

    def project_info_for_cwd(self, cwd: str) -> ProjectInfo:
        key = str(cwd or "")
        cached = self._project_info_cache.get(key)
        if cached is None:
            cached = git_project_info(key)
            self._project_info_cache[key] = cached
        return cached

    def parse_file(
        self,
        path: Path,
        source: str,
        log_source: CodexLogSource | None = None,
    ) -> tuple[dict[str, Any], dict[str, Any]]:
        log_source = log_source or self.codex_sources[0]
        uid = hashlib.sha1(f"{log_source.id}:{path.resolve()}".encode("utf-8", errors="replace")).hexdigest()[:16]
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
                        if text and not first_user_prompt and not is_synthetic_user_context(text):
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
                    if isinstance(message, str) and not first_user_prompt and not is_synthetic_user_context(message):
                        first_user_prompt = clean_text(message, 260)

        if not session_id:
            match = re.search(r"rollout-[^-]+-[^-]+-(.+?)\.jsonl$", path.name)
            session_id = match.group(1) if match else uid

        if not title:
            title = first_user_prompt or folder_name_from_path(cwd) or path.stem

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
        project_info = self.project_info_for_cwd(cwd)

        detail: dict[str, Any] = {
            "uid": uid,
            "session_id": session_id,
            "title": title,
            "source": source,
            "environment": log_source.label,
            "environment_id": log_source.id,
            "is_remote": False,
            "remote_device_short_code": "",
            "remote_imported_at": "",
            "remote_exported_at": "",
            "codex_home": str(log_source.codex_home),
            "path": str(path.resolve()),
            "file_size": path.stat().st_size,
            "line_count": line_count,
            "parse_errors": parse_errors,
            "created_at": created_at or start_at,
            "start_at": start_at,
            "end_at": end_at,
            "updated_at": utc_from_mtime(path),
            "cwd": cwd,
            "project": project_info.project,
            "project_root": project_info.project_root,
            "workspace_root": project_info.workspace_root,
            "project_branch": project_info.project_branch,
            "is_git_worktree": project_info.is_git_worktree,
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

        summary = {key: detail[key] for key in SUMMARY_KEYS}
        return summary, detail

    def build_summary(self, sessions: list[dict[str, Any]]) -> dict[str, Any]:
        return self.build_summary_static(sessions)

    @staticmethod
    def build_summary_static(sessions: list[dict[str, Any]]) -> dict[str, Any]:
        totals = zero_usage()
        by_model: dict[str, dict[str, Any]] = {}
        by_project: dict[str, dict[str, Any]] = {}
        by_day: dict[str, dict[str, Any]] = {}
        by_environment: dict[str, dict[str, Any]] = {}
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

            project_key = str(session.get("project_root") or session.get("project") or "unknown")
            project = str(session.get("project") or folder_name_from_path(project_key) or "unknown")
            by_project.setdefault(
                project_key,
                {"project": project, "project_root": project_key, "sessions": 0, "usage": zero_usage()},
            )
            by_project[project_key]["sessions"] += 1
            by_project[project_key]["usage"] = add_usage(by_project[project_key]["usage"], usage)

            environment_id = str(session.get("environment_id") or "local")
            environment = str(session.get("environment") or environment_id)
            by_environment.setdefault(
                environment_id,
                {"id": environment_id, "label": environment, "sessions": 0, "usage": zero_usage()},
            )
            by_environment[environment_id]["sessions"] += 1
            by_environment[environment_id]["usage"] = add_usage(by_environment[environment_id]["usage"], usage)

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
            "by_environment": sorted(by_environment.values(), key=lambda row: row["usage"]["total_tokens"], reverse=True),
            "by_day": sorted(by_day.values(), key=lambda row: row["date"]),
            "top_session_uid": sessions[0]["uid"] if sessions else None,
        }

    def get_detail(
        self,
        uid: str,
        period: str | None = None,
        start_date: str | None = None,
        end_date: str | None = None,
    ) -> dict[str, Any] | None:
        snapshot = self.scan(period, start_date, end_date)
        return snapshot["details_by_uid"].get(uid)

    def export_snapshot_payload(self) -> dict[str, Any]:
        snapshot = self.scan("all", include_remotes=False)
        device_code = self.remote_store.current_device_code if self.remote_store else current_device_short_code()
        now = utc_iso(dt.datetime.now(dt.UTC))
        return {
            "schema": SNAPSHOT_SCHEMA,
            "version": SNAPSHOT_VERSION,
            "exported_at": now,
            "device": {
                "short_code": device_code,
                "label": default_device_label(),
                "platform": platform.system() or "",
                "hostname": platform.node() or "",
            },
            "snapshot": {
                "generated_at": snapshot["generated_at"],
                "codex_home": snapshot["codex_home"],
                "codex_sources": snapshot.get("codex_sources", []),
                "summary": snapshot["summary"],
                "sessions": snapshot["sessions"],
                "details_by_uid": snapshot["details_by_uid"],
                "daily_usage": snapshot.get("daily_usage", []),
            },
        }

    def export_snapshot_json(self) -> str:
        return json.dumps(self.export_snapshot_payload(), ensure_ascii=False, indent=2)


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
      display: grid;
      grid-template-columns: minmax(220px, 1fr) auto minmax(220px, 1fr);
      align-items: center;
      gap: 16px;
    }
    .brand {
      min-width: 0;
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
      justify-self: end;
    }
    .period-wrap {
      position: relative;
      justify-self: center;
    }
    .period-toggle {
      display: grid;
      grid-template-columns: repeat(7, minmax(48px, 1fr));
      gap: 3px;
      min-height: 38px;
      padding: 3px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #fff;
      min-width: 458px;
    }
    .period-option {
      min-height: 30px;
      border: 0;
      border-radius: 6px;
      background: transparent;
      color: var(--muted);
      font-weight: 700;
      padding: 0 10px;
      white-space: nowrap;
    }
    .period-option.active {
      background: var(--accent);
      color: #fff;
      box-shadow: 0 1px 4px rgba(15, 123, 99, 0.22);
    }
    .calendar-popover {
      position: absolute;
      top: calc(100% + 8px);
      left: 50%;
      transform: translateX(-50%);
      width: 360px;
      padding: 12px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #fff;
      box-shadow: 0 18px 40px rgba(26, 36, 32, 0.18);
      z-index: 20;
    }
    .calendar-popover[hidden] {
      display: none;
    }
    .modal-backdrop[hidden] {
      display: none;
    }
    .modal-backdrop {
      position: fixed;
      inset: 0;
      z-index: 40;
      display: grid;
      place-items: center;
      padding: 24px;
      background: rgba(23, 32, 29, 0.36);
    }
    .modal {
      width: min(720px, 100%);
      max-height: min(720px, calc(100vh - 48px));
      overflow: auto;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #fff;
      box-shadow: 0 20px 60px rgba(23, 32, 29, 0.22);
    }
    .modal-head,
    .modal-actions {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      padding: 14px 16px;
      border-bottom: 1px solid var(--line);
    }
    .modal-actions {
      justify-content: flex-end;
      border-top: 1px solid var(--line);
      border-bottom: 0;
    }
    .modal-head h2 {
      margin: 0;
      font-size: 16px;
      line-height: 1.3;
    }
    .modal-body {
      padding: 16px;
      display: grid;
      gap: 12px;
    }
    .remote-table {
      width: 100%;
      min-width: 0;
      table-layout: fixed;
      font-size: 13px;
    }
    .remote-table th:nth-child(1) { width: 18%; }
    .remote-table th:nth-child(2) { width: 18%; }
    .remote-table th:nth-child(3) { width: 9%; }
    .remote-table th:nth-child(4) { width: 15%; }
    .remote-table th:nth-child(5) { width: 15%; }
    .remote-table th:nth-child(6) { width: 25%; }
    .remote-table th,
    .remote-table td {
      position: static;
      padding: 8px;
      vertical-align: middle;
    }
    .remote-table th {
      text-align: left;
    }
    .remote-table .remote-count {
      font-variant-numeric: tabular-nums;
      text-align: left;
      white-space: nowrap;
    }
    .remote-table .remote-date,
    .remote-table .remote-code {
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .remote-table .remote-action-cell {
      text-align: right;
    }
    .remote-actions {
      display: flex;
      gap: 6px;
      justify-content: flex-end;
      flex-wrap: nowrap;
      white-space: nowrap;
    }
    .remote-actions button {
      min-height: 30px;
      padding: 0 8px;
    }
    .inline-form {
      display: grid;
      gap: 8px;
    }
    .inline-form label {
      color: var(--muted);
      font-size: 12px;
      font-weight: 700;
    }
    .inline-form input {
      width: 100%;
      padding: 0 10px;
    }
    .status-line {
      color: var(--muted);
      font-size: 12px;
      line-height: 1.5;
    }
    .status-line.error-text {
      color: var(--danger);
    }
    .danger {
      color: var(--danger);
      border-color: #f1b4ae;
      background: #fff7f6;
    }
    .calendar-head,
    .calendar-actions {
      display: grid;
      grid-template-columns: 36px minmax(0, 1fr) 36px;
      gap: 8px;
      align-items: center;
      margin-bottom: 10px;
    }
    .calendar-title {
      text-align: center;
      font-weight: 750;
      font-size: 14px;
    }
    .calendar-actions {
      grid-template-columns: 1fr 1fr;
      margin: 10px 0 0;
    }
    .calendar-grid {
      display: grid;
      grid-template-columns: repeat(7, minmax(0, 1fr));
      gap: 4px;
    }
    .calendar-weekday {
      color: var(--muted);
      font-size: 11px;
      font-weight: 700;
      text-align: center;
      padding: 4px 0;
    }
    .calendar-day {
      display: grid;
      grid-template-rows: 16px 14px;
      gap: 2px;
      min-height: 42px;
      padding: 4px 2px;
      border-radius: 6px;
      font-size: 12px;
      line-height: 1;
      text-align: center;
    }
    .calendar-day.outside {
      color: #a3aca8;
      background: #fbfcfc;
    }
    .calendar-day.in-range {
      background: #eef8f4;
      border-color: #99c9bb;
    }
    .calendar-day.range-edge {
      background: var(--accent);
      border-color: var(--accent);
      color: #fff;
    }
    .calendar-day:disabled {
      color: #b8c0bd;
      cursor: default;
      background: #f8faf9;
    }
    .calendar-usage {
      color: var(--muted);
      font-size: 10px;
      line-height: 1.2;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .calendar-day.range-edge .calendar-usage {
      color: rgba(255, 255, 255, 0.84);
    }
    .lang-toggle {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 3px;
      min-height: 36px;
      padding: 3px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #fff;
    }
    .lang-option {
      min-height: 28px;
      border: 0;
      border-radius: 6px;
      background: transparent;
      color: var(--muted);
      font-weight: 700;
      padding: 0 9px;
    }
    .lang-option.active {
      background: #17201d;
      color: #fff;
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
      grid-template-columns: minmax(220px, 1fr) 140px 150px 170px 210px 130px;
      gap: 10px;
      margin-bottom: 16px;
    }
    input, select {
      width: 100%;
      padding: 0 10px;
    }
    .sort-toggle {
      display: grid;
      grid-template-columns: 1fr 1fr 1fr;
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
    tr.project-group-row td {
      background: #fbfcfc;
      padding: 10px 12px;
      border-top: 1px solid var(--line);
      border-bottom: 1px solid var(--line);
    }
    tr.project-group-row:hover td {
      background: #f4f8f6;
    }
    .project-title-cell {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 12px;
      align-items: center;
      min-width: 0;
    }
    .project-toggle {
      display: flex;
      align-items: center;
      gap: 12px;
      min-width: 0;
      width: 100%;
      min-height: 30px;
      padding: 0;
      border: 0;
      background: transparent;
      text-align: left;
    }
    .project-folder-icon {
      width: 18px;
      height: 18px;
      flex: 0 0 auto;
      color: var(--accent);
    }
    .project-name {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-weight: 750;
    }
    .project-meta {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      gap: 8px;
      color: var(--muted);
      font-size: 12px;
      white-space: nowrap;
    }
    .project-summary-cell {
      color: var(--muted);
      font-size: 12px;
      text-align: right;
      white-space: nowrap;
    }
    .project-session-row .title-cell {
      padding-left: 42px;
    }
    .project-session-row .title-line {
      gap: 8px;
      margin-bottom: 0;
    }
    .project-session-row .title-main {
      margin-bottom: 0;
    }
    .project-session-row .title-sub {
      display: none;
    }
    .project-more-row td {
      background: #fff;
      padding: 8px 8px 10px 42px;
    }
    .project-more-row:hover td {
      background: #fff;
    }
    .project-more-btn {
      min-height: 30px;
      padding: 0 10px;
      color: var(--accent);
      border-color: #99c9bb;
      background: #eef8f4;
      font-weight: 700;
    }
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
    .badge.env { color: var(--accent-3); border-color: #a9c0df; background: #eef4fb; }
    .badge.env.wsl { color: var(--accent); border-color: #99c9bb; background: #eef8f4; }
    .badge.env.windows { color: var(--accent-3); border-color: #a9c0df; background: #eef4fb; }
    .badge.env.remote { color: #7a4c12; border-color: #e3c27a; background: #fff8e5; }
    .badge.branch {
      width: 24px;
      min-width: 24px;
      min-height: 22px;
      justify-content: center;
      padding: 2px;
      color: #5b4aa0;
      border-color: #c3b8ee;
      background: #f4f1ff;
    }
    .branch-icon {
      width: 13px;
      height: 13px;
      flex: 0 0 auto;
    }
    .remote-mark {
      margin-right: 4px;
      font-weight: 800;
      line-height: 1;
    }
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
      .header-inner { grid-template-columns: 1fr; align-items: flex-start; }
      .period-wrap { justify-self: start; width: 100%; max-width: 620px; }
      .period-toggle { width: 100%; min-width: 0; }
      .toolbar { justify-self: start; justify-content: flex-start; }
      .metrics { grid-template-columns: repeat(3, minmax(130px, 1fr)); }
      .controls { grid-template-columns: repeat(3, minmax(0, 1fr)); }
      .layout { grid-template-columns: 1fr; }
      .details { position: static; max-height: none; }
      .table-wrap { max-height: none; }
    }
    @media (max-width: 760px) {
      .header-inner { grid-template-columns: 1fr; padding: 16px; }
      main { padding: 16px; }
      .toolbar { justify-content: flex-start; }
      .period-toggle { grid-template-columns: repeat(7, minmax(0, 1fr)); }
      .period-option { padding: 0 6px; }
      .calendar-popover { left: 0; transform: none; width: min(358px, calc(100vw - 32px)); }
      .metrics { grid-template-columns: repeat(2, minmax(120px, 1fr)); }
      .controls { grid-template-columns: 1fr; }
      .project-title-cell { grid-template-columns: minmax(0, 1fr) auto; gap: 8px; }
      .project-meta { justify-content: flex-start; flex-wrap: wrap; }
      .project-session-row .title-cell { padding-left: 42px; }
      .bar-row { grid-template-columns: 1fr; gap: 4px; }
      .kv { grid-template-columns: 1fr; gap: 2px; }
    }
  </style>
</head>
<body>
  <header>
    <div class="header-inner">
      <div class="brand">
        <h1>Codex Usage Dashboard</h1>
        <div class="subtitle" id="codexHome" data-i18n="subtitle">读取本地 Codex 会话日志</div>
      </div>
      <div class="period-wrap" id="periodWrap">
        <div class="period-toggle" aria-label="统计区间" data-i18n-aria="period">
          <button class="period-option active" data-period-button="today" data-i18n="periodToday" type="button">今日</button>
          <button class="period-option" data-period-button="7d" data-i18n="period7d" type="button">7日</button>
          <button class="period-option" data-period-button="30d" data-i18n="period30d" type="button">30日</button>
          <button class="period-option" data-period-button="week" data-i18n="periodWeek" type="button">本周</button>
          <button class="period-option" data-period-button="month" data-i18n="periodMonth" type="button">本月</button>
          <button class="period-option" data-period-button="all" data-i18n="periodAll" type="button">全部</button>
          <button class="period-option" id="calendarBtn" data-i18n="calendarButton" type="button">日期</button>
        </div>
        <div class="calendar-popover" id="calendarPopover" hidden></div>
      </div>
      <div class="toolbar">
        <div class="lang-toggle" aria-label="语言" data-i18n-aria="language">
          <button class="lang-option active" data-lang-button="zh" type="button">中文</button>
          <button class="lang-option" data-lang-button="en" type="button">EN</button>
        </div>
        <button id="refreshBtn" class="primary" title="重新扫描本地日志" data-i18n="refresh" data-i18n-title="refreshTitle">刷新</button>
        <button id="remoteBtn" title="导入远程数据" data-i18n="importRemote" data-i18n-title="importRemoteTitle">导入远程数据</button>
        <button id="snapshotExportBtn" title="导出当前设备快照" data-i18n="exportSnapshot" data-i18n-title="exportSnapshotTitle">导出快照</button>
      </div>
    </div>
  </header>

  <input id="remoteFileInput" type="file" accept="application/json,.json" hidden>
  <div class="modal-backdrop" id="remoteModal" hidden>
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="remoteModalTitle">
      <div class="modal-head">
        <h2 id="remoteModalTitle" data-i18n="remoteData">远程数据</h2>
        <button id="remoteCloseBtn" type="button" title="关闭" data-i18n-title="close">关闭</button>
      </div>
      <div class="modal-body" id="remoteModalBody"></div>
      <div class="modal-actions">
        <button id="remoteImportBtn" class="primary" type="button" data-i18n="importRemote">导入远程数据</button>
      </div>
    </div>
  </div>

  <main>
    <div class="metrics" id="metrics"></div>

    <div class="controls">
      <input id="searchInput" type="search" placeholder="搜索标题、项目、路径、模型、环境" data-i18n-placeholder="searchPlaceholder">
      <select id="environmentFilter" title="环境" data-i18n-title="environment">
        <option value="all">全部环境</option>
      </select>
      <select id="sourceFilter" title="状态" data-i18n-title="source">
        <option value="all">全部状态</option>
        <option value="active">当前会话</option>
        <option value="archived">归档会话</option>
      </select>
      <select id="modelFilter" title="模型" data-i18n-title="model">
        <option value="all">全部模型</option>
      </select>
      <div class="sort-toggle" aria-label="视图" data-i18n-aria="view">
        <button class="sort-option active" data-view-button="project" data-i18n="projectTab" type="button">项目</button>
        <button class="sort-option" data-view-button="recent" data-i18n="recent" type="button">最近</button>
        <button class="sort-option" data-view-button="total" data-i18n="total" type="button">总量</button>
      </div>
      <select id="limitSelect" title="显示数量" data-i18n-title="limitTitle">
        <option value="50">前 50</option>
        <option value="100">前 100</option>
        <option value="all">全部</option>
      </select>
    </div>

    <div class="layout">
      <section class="panel">
        <div class="panel-title">
          <h2 data-i18n="conversations">对话</h2>
          <div class="count" id="resultCount" data-i18n="loading">加载中</div>
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
                <th data-sort="title" data-i18n="conversation">对话</th>
                <th data-sort="total_tokens" data-i18n="total">总量</th>
                <th data-sort="output_tokens" data-i18n="output">输出</th>
                <th data-sort="estimated_cost_usd" data-i18n="cost">花费</th>
                <th data-sort="cached_input_percent" data-i18n="cacheHit">缓存命中</th>
                <th data-sort="turn_count" data-i18n="turns">轮次</th>
                <th data-sort="model" data-i18n="model">模型</th>
                <th data-sort="effort" data-i18n="reasoningEffort">推理强度</th>
              </tr>
            </thead>
            <tbody id="sessionRows">
              <tr><td colspan="8" class="empty" data-i18n="loading">加载中</td></tr>
            </tbody>
          </table>
        </div>
      </section>

      <aside class="panel details">
        <div class="panel-title">
          <h2 data-i18n="conversationDetails">对话明细</h2>
          <div class="count" id="detailStatus" data-i18n="notSelected">未选择</div>
        </div>
        <div class="details-body" id="detailsBody">
          <div class="empty" data-i18n="selectRow">点击左侧任意一行查看 token 明细和时间线。</div>
        </div>
      </aside>
    </div>
  </main>

  <script>
    const state = {
      sessions: [],
      summary: null,
      codexHome: '',
      codexSources: [],
      generatedAt: '',
      selectedUid: null,
      viewMode: 'project',
      sortKey: 'end_at',
      sortDir: 'desc',
      projectExpanded: {},
      projectShowAll: {},
      search: '',
      environment: 'all',
      source: 'all',
      model: 'all',
      limit: '50',
      period: 'today',
      customStartDate: '',
      customEndDate: '',
      periodCache: new Map(),
      dailyUsage: [],
      remotes: [],
      currentDeviceShortCode: '',
      pendingRemoteSnapshot: null,
      calendarOpen: false,
      calendarMonth: '',
      calendarDraftStart: '',
      calendarDraftEnd: '',
      lang: 'zh',
      loading: false,
      reloadAfterLoad: false,
    };

    const tokenKeys = ['input_tokens', 'cached_input_tokens', 'output_tokens', 'reasoning_output_tokens', 'total_tokens'];
    const projectPreviewLimit = 5;

    const I18N = {
      zh: {
        subtitle: '读取本地 Codex 会话日志',
        refresh: '刷新',
        refreshTitle: '重新扫描本地日志',
        importRemote: '导入远程数据',
        manageRemote: '管理远程数据',
        importRemoteTitle: '导入或管理其他设备导出的快照',
        exportSnapshot: '导出快照',
        exportSnapshotTitle: '导出当前设备的 Cousash JSON 快照',
        remoteData: '远程数据',
        close: '关闭',
        remoteEmpty: '还没有导入远程设备数据。',
        remoteImportHelp: '选择另一台设备导出的 Cousash JSON 快照文件。',
        remoteNeedLabel: '这是新的远程设备，请输入显示名称。',
        remoteCurrentWarning: '这个文件来自当前设备，导入后可能与本机实时统计重复。是否仍然导入为远程数据？',
        remoteDeleteConfirm: '删除后无法恢复。确定删除这台远程设备的数据吗？',
        remoteImported: '远程数据已导入。',
        remoteDeleted: '远程数据已删除。',
        remoteRenamed: '设备名称已更新。',
        remoteImportFailed: '导入失败：{message}',
        remoteName: '设备名',
        remoteCode: '设备短码',
        remoteSessions: '会话',
        remoteUpdated: '快照时间',
        remoteImportedAt: '导入时间',
        remoteActions: '操作',
        remoteUpdate: '更新',
        remoteRename: '重命名',
        remoteDelete: '删除',
        remoteSave: '保存',
        remoteCancel: '取消',
        remoteDevicePrefix: '远程',
        language: '语言',
        period: '统计区间',
        periodToday: '今日',
        period7d: '7日',
        period30d: '30日',
        periodWeek: '本周',
        periodMonth: '本月',
        periodAll: '全部',
        periodCustom: '{start}-{end}',
        calendarButton: '日期',
        calendarTitle: '选择日期',
        calendarApply: '应用',
        calendarCancel: '取消',
        calendarPrev: '上月',
        calendarNext: '下月',
        calendarWeekdays: ['一', '二', '三', '四', '五', '六', '日'],
        searchPlaceholder: '搜索标题、项目、路径、模型、环境',
        environment: '环境',
        environmentAll: '全部环境',
        source: '状态',
        sourceAll: '全部状态',
        sourceActive: '当前会话',
        sourceArchived: '归档会话',
        model: '模型',
        allModels: '全部模型',
        sort: '排序',
        view: '视图',
        projectTab: '项目',
        recent: '最近',
        total: '总量',
        limitTitle: '显示数量',
        limit50: '前 50',
        limit100: '前 100',
        limitAll: '全部',
        conversations: '对话',
        conversation: '对话',
        output: '输出',
        cost: '花费',
        cacheHit: '缓存命中',
        turns: '轮次',
        reasoningEffort: '推理强度',
        conversationDetails: '对话明细',
        loading: '加载中',
        notSelected: '未选择',
        selectRow: '点击左侧任意一行查看 token 明细和时间线。',
        scanning: '扫描中',
        scanned: '扫描',
        loadFailed: '加载失败：{message}',
        noMatches: '没有匹配的会话',
        projectRowCount: '{projects} 个工作区 · {sessions} 条对话',
        projectConversationCount: '{count} 条对话',
        projectLatest: '最近 {time}',
        showMoreConversations: '展开剩余 {count} 条',
        showFewerConversations: '收起到 5 条',
        collapseProject: '收起工作区',
        expandProject: '展开工作区',
        priceKnown: '按公开 API 标准价格估算花费',
        priceUnknown: '没有匹配到公开模型价格',
        archived: '归档',
        justNow: '刚刚',
        minutes: '{value} 分钟',
        hours: '{value} 小时',
        days: '{value} 天',
        months: '{value} 个月',
        years: '{value} 年',
        seconds: '{value} 秒',
        minutesSeconds: '{minutes} 分 {seconds} 秒',
        hoursMinutes: '{hours} 小时 {minutes} 分',
        metricSessions: '会话数',
        metricSessionsHint: '当前 {active} · 归档 {archived}',
        metricSessionsHintWithEnvs: '当前 {active} · 归档 {archived} · {envs}',
        metricTotalTokens: '总 tokens',
        metricPeriodTotalTokens: '{period}总 tokens',
        metricCost: '估算价格',
        metricCostHint: '{count} 个会话可估算',
        metricInput: '输入 tokens',
        metricCached: '缓存输入',
        metricOutput: '输出 tokens',
        rowCount: '{count} 条',
        detailsLoading: '加载中',
        detailFailed: '明细加载失败：{message}',
        failed: '失败',
        countEvents: '{count} 次计数',
        turnSuffix: '{count} 轮',
        input: '输入',
        cached: '缓存',
        reasoning: '推理',
        reasoningCostTitle: '推理花费按输出单价估算，已包含在输出花费中',
        cumulativeChart: '累计曲线',
        metadata: '元数据',
        totalTokens: '总 tokens',
        cachePercent: '缓存占比',
        time: '时间',
        totalDuration: '总耗时',
        ttftAvg: 'TTFT 均值',
        project: '项目',
        projectRoot: '项目根目录',
        workspaceRoot: '工作树目录',
        branch: '分支',
        worktree: '工作树',
        cwd: '工作目录',
        logFile: '日志文件',
        codexHome: 'Codex home',
        firstUserPrompt: '首条用户消息',
        lastReplySummary: '最后回复摘要',
        toolCalls: '工具调用',
        tool: '工具',
        count: '次数',
        noToolCalls: '没有记录到工具调用。',
        timelineDetails: '每次计数明细',
        timelineTime: '时间',
        timelineTotal: '本次总量',
        noTimeline: '这个会话没有 token_count.info 记录。',
        noCurve: '没有曲线数据',
        countPoints: '{count} 次计数',
      },
      en: {
        subtitle: 'Reading local Codex session logs',
        refresh: 'Refresh',
        refreshTitle: 'Rescan local logs',
        importRemote: 'Import Remote',
        manageRemote: 'Manage Remote',
        importRemoteTitle: 'Import or manage snapshots exported from other devices',
        exportSnapshot: 'Export Snapshot',
        exportSnapshotTitle: 'Export this device as a Cousash JSON snapshot',
        remoteData: 'Remote Data',
        close: 'Close',
        remoteEmpty: 'No remote device data has been imported.',
        remoteImportHelp: 'Choose a Cousash JSON snapshot exported on another device.',
        remoteNeedLabel: 'This is a new remote device. Enter a display name.',
        remoteCurrentWarning: 'This file is from the current device. Importing it may duplicate local realtime data. Import it as remote data anyway?',
        remoteDeleteConfirm: 'This cannot be undone. Delete this remote device data?',
        remoteImported: 'Remote data imported.',
        remoteDeleted: 'Remote data deleted.',
        remoteRenamed: 'Device name updated.',
        remoteImportFailed: 'Import failed: {message}',
        remoteName: 'Device',
        remoteCode: 'Short code',
        remoteSessions: 'Sessions',
        remoteUpdated: 'Snapshot',
        remoteImportedAt: 'Imported',
        remoteActions: 'Actions',
        remoteUpdate: 'Update',
        remoteRename: 'Rename',
        remoteDelete: 'Delete',
        remoteSave: 'Save',
        remoteCancel: 'Cancel',
        remoteDevicePrefix: 'Remote',
        language: 'Language',
        period: 'Range',
        periodToday: 'Today',
        period7d: '7d',
        period30d: '30d',
        periodWeek: 'Week',
        periodMonth: 'Month',
        periodAll: 'All',
        periodCustom: '{start}-{end}',
        calendarButton: 'Dates',
        calendarTitle: 'Select Dates',
        calendarApply: 'Apply',
        calendarCancel: 'Cancel',
        calendarPrev: 'Prev',
        calendarNext: 'Next',
        calendarWeekdays: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'],
        searchPlaceholder: 'Search title, project, path, model, or environment',
        environment: 'Environment',
        environmentAll: 'All environments',
        source: 'Status',
        sourceAll: 'All statuses',
        sourceActive: 'Active',
        sourceArchived: 'Archived',
        model: 'Model',
        allModels: 'All models',
        sort: 'Sort',
        view: 'View',
        projectTab: 'Project',
        recent: 'Recent',
        total: 'Total',
        limitTitle: 'Rows',
        limit50: 'Top 50',
        limit100: 'Top 100',
        limitAll: 'All',
        conversations: 'Conversations',
        conversation: 'Conversation',
        output: 'Output',
        cost: 'Cost',
        cacheHit: 'Cache hit',
        turns: 'Turns',
        reasoningEffort: 'Reasoning',
        conversationDetails: 'Details',
        loading: 'Loading',
        notSelected: 'No selection',
        selectRow: 'Select a row to inspect token details and timeline.',
        scanning: 'Scanning',
        scanned: 'scanned',
        loadFailed: 'Load failed: {message}',
        noMatches: 'No matching conversations',
        projectRowCount: '{projects} workspaces · {sessions} conversations',
        projectConversationCount: '{count} conversations',
        projectLatest: 'Latest {time}',
        showMoreConversations: 'Show {count} more',
        showFewerConversations: 'Show first 5',
        collapseProject: 'Collapse workspace',
        expandProject: 'Expand workspace',
        priceKnown: 'Estimated from public API prices',
        priceUnknown: 'No matching public model price',
        archived: 'Archived',
        justNow: 'just now',
        minutes: '{value} min',
        hours: '{value} hr',
        days: '{value} days',
        months: '{value} mo',
        years: '{value} yr',
        seconds: '{value} sec',
        minutesSeconds: '{minutes} min {seconds} sec',
        hoursMinutes: '{hours} hr {minutes} min',
        metricSessions: 'Sessions',
        metricSessionsHint: 'Active {active} · Archived {archived}',
        metricSessionsHintWithEnvs: 'Active {active} · Archived {archived} · {envs}',
        metricTotalTokens: 'Total tokens',
        metricPeriodTotalTokens: '{period} total tokens',
        metricCost: 'Estimated cost',
        metricCostHint: '{count} sessions priced',
        metricInput: 'Input tokens',
        metricCached: 'Cached input',
        metricOutput: 'Output tokens',
        rowCount: '{count} rows',
        detailsLoading: 'Loading',
        detailFailed: 'Detail load failed: {message}',
        failed: 'Failed',
        countEvents: '{count} counts',
        turnSuffix: '{count} turns',
        input: 'Input',
        cached: 'Cached',
        reasoning: 'Reasoning',
        reasoningCostTitle: 'Reasoning cost is estimated at the output rate and is included in output cost.',
        cumulativeChart: 'Cumulative Chart',
        metadata: 'Metadata',
        totalTokens: 'Total tokens',
        cachePercent: 'Cache rate',
        time: 'Time',
        totalDuration: 'Total duration',
        ttftAvg: 'Avg TTFT',
        project: 'Project',
        projectRoot: 'Project root',
        workspaceRoot: 'Worktree dir',
        branch: 'Branch',
        worktree: 'Worktree',
        cwd: 'Working dir',
        logFile: 'Log file',
        codexHome: 'Codex home',
        firstUserPrompt: 'First User Message',
        lastReplySummary: 'Last Reply Summary',
        toolCalls: 'Tool Calls',
        tool: 'Tool',
        count: 'Count',
        noToolCalls: 'No tool calls recorded.',
        timelineDetails: 'Token Count Events',
        timelineTime: 'Time',
        timelineTotal: 'Event total',
        noTimeline: 'This conversation has no token_count.info records.',
        noCurve: 'No chart data',
        countPoints: '{count} counts',
      },
    };

    function t(key, vars = {}) {
      const template = (I18N[state.lang] && I18N[state.lang][key]) || I18N.zh[key] || key;
      return template.replace(/\{(\w+)\}/g, (_, name) => String(vars[name] ?? ''));
    }

    function locale() {
      return state.lang === 'en' ? 'en-US' : 'zh-CN';
    }

    function applyStaticText() {
      document.documentElement.lang = state.lang === 'en' ? 'en' : 'zh-CN';
      document.querySelectorAll('[data-i18n]').forEach(el => {
        el.textContent = t(el.dataset.i18n);
      });
      document.querySelectorAll('[data-i18n-title]').forEach(el => {
        el.title = t(el.dataset.i18nTitle);
      });
      document.querySelectorAll('[data-i18n-placeholder]').forEach(el => {
        el.placeholder = t(el.dataset.i18nPlaceholder);
      });
      document.querySelectorAll('[data-i18n-aria]').forEach(el => {
        el.setAttribute('aria-label', t(el.dataset.i18nAria));
      });
      document.querySelectorAll('[data-lang-button]').forEach(button => {
        button.classList.toggle('active', button.dataset.langButton === state.lang);
      });
      updateRemoteButton();
      updatePeriodButtons();
      if (state.generatedAt) {
        document.getElementById('codexHome').textContent = `${state.codexHome || ''} · ${fmtDate(state.generatedAt)} ${t('scanned')}`;
      }
    }

    function environmentsFromSessions() {
      const seen = new Map();
      state.sessions.forEach(row => {
        const id = row.environment_id || row.environment || 'local';
        if (!seen.has(id)) seen.set(id, { id, label: row.environment || id });
      });
      return Array.from(seen.values());
    }

    function populateEnvironmentFilter() {
      const select = document.getElementById('environmentFilter');
      const oldValue = select.value || state.environment;
      const sources = state.codexSources.length ? state.codexSources : environmentsFromSessions();
      select.innerHTML = `<option value="all">${escapeHtml(t('environmentAll'))}</option>` + sources
        .map(source => `<option value="${escapeHtml(source.id)}">${escapeHtml(source.label)}</option>`)
        .join('');
      const values = sources.map(source => source.id);
      select.value = values.includes(oldValue) ? oldValue : 'all';
      state.environment = select.value;
    }

    function populateSourceFilter() {
      const select = document.getElementById('sourceFilter');
      const oldValue = select.value || state.source;
      select.innerHTML = [
        ['all', t('sourceAll')],
        ['active', t('sourceActive')],
        ['archived', t('sourceArchived')],
      ].map(([value, label]) => `<option value="${value}">${escapeHtml(label)}</option>`).join('');
      select.value = ['all', 'active', 'archived'].includes(oldValue) ? oldValue : 'all';
      state.source = select.value;
    }

    function populateLimitSelect() {
      const select = document.getElementById('limitSelect');
      const oldValue = select.value || state.limit;
      select.innerHTML = [
        ['50', t('limit50')],
        ['100', t('limit100')],
        ['all', t('limitAll')],
      ].map(([value, label]) => `<option value="${value}">${escapeHtml(label)}</option>`).join('');
      select.value = ['50', '100', 'all'].includes(oldValue) ? oldValue : '50';
      state.limit = select.value;
    }

    function setLanguage(lang) {
      state.lang = lang === 'en' ? 'en' : 'zh';
      applyStaticText();
      populateEnvironmentFilter();
      populateSourceFilter();
      populateLimitSelect();
      populateModelFilter();
      renderAll();
      if (state.selectedUid) showDetails(state.selectedUid, false);
    }

    function usageOf(row) {
      return row && row.total_token_usage ? row.total_token_usage : {};
    }

    function tokenValue(row, key) {
      return Number(usageOf(row)[key] || 0);
    }

    function zeroClientUsage() {
      return Object.fromEntries(tokenKeys.map(key => [key, 0]));
    }

    function addClientUsage(left, right) {
      const usage = {};
      tokenKeys.forEach(key => {
        usage[key] = Number(left?.[key] || 0) + Number(right?.[key] || 0);
      });
      return usage;
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
      return Number(n || 0).toLocaleString(locale());
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
      return date.toLocaleString(locale(), { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
    }

    function fmtRelativeTime(value) {
      if (!value) return '';
      const date = new Date(value);
      if (Number.isNaN(date.getTime())) return '';
      const diffSeconds = Math.max(0, Math.floor((Date.now() - date.getTime()) / 1000));
      if (diffSeconds < 60) return t('justNow');
      const minutes = Math.floor(diffSeconds / 60);
      if (minutes < 60) return t('minutes', { value: minutes });
      const hours = Math.floor(minutes / 60);
      if (hours < 48) return t('hours', { value: hours });
      const days = Math.floor(hours / 24);
      if (days < 30) return t('days', { value: days });
      const months = Math.floor(days / 30);
      if (months < 12) return t('months', { value: months });
      return t('years', { value: Math.floor(months / 12) });
    }

    function fmtDuration(ms) {
      if (!ms) return '';
      const seconds = Math.round(ms / 1000);
      if (seconds < 60) return t('seconds', { value: seconds });
      const minutes = Math.floor(seconds / 60);
      const rest = seconds % 60;
      if (minutes < 60) return t('minutesSeconds', { minutes, seconds: rest });
      const hours = Math.floor(minutes / 60);
      return t('hoursMinutes', { hours, minutes: minutes % 60 });
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

    function rowTime(row) {
      const date = new Date(row.end_at || row.updated_at || row.start_at || 0);
      const value = date.getTime();
      return Number.isFinite(value) ? value : 0;
    }

    function projectName(row) {
      return row.project || shortPath(row.cwd) || row.session_id || t('projectTab');
    }

    function workspaceKey(row) {
      return row.project_root || row.cwd || row.project || row.path || row.session_id || 'unknown';
    }

    function projectKey(row) {
      const environment = row.environment_id || row.environment || 'local';
      return `${encodeURIComponent(environment)}::${encodeURIComponent(workspaceKey(row))}`;
    }

    function badgeClass(value) {
      return String(value || 'local').toLowerCase().replace(/[^a-z0-9_-]+/g, '-');
    }

    function environmentBadge(row) {
      const label = row.environment || '';
      if (!label) return '';
      const remote = row.is_remote ? `<span class="remote-mark" title="${escapeHtml(t('remoteDevicePrefix'))}">↗</span>` : '';
      return `<span class="badge env ${row.is_remote ? 'remote' : ''} ${escapeHtml(badgeClass(row.environment_id || label))}">${remote}${escapeHtml(label)}</span>`;
    }

    function folderIcon() {
      return `
        <svg class="project-folder-icon" aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.2a2 2 0 0 1-1.6-.8l-.4-.6A2 2 0 0 0 9.2 4H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2Z"></path>
        </svg>
      `;
    }

    function branchIcon() {
      return `
        <svg class="branch-icon" aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="6" cy="6" r="3"></circle>
          <circle cx="18" cy="18" r="3"></circle>
          <path d="M6 9v3a6 6 0 0 0 6 6h3"></path>
          <path d="M6 9v12"></path>
        </svg>
      `;
    }

    function branchBadge(row) {
      if (!row.is_git_worktree) return '';
      const label = row.project_branch || shortPath(row.workspace_root || row.cwd) || t('worktree');
      const title = row.workspace_root || row.cwd
        ? `${label} · ${row.workspace_root || row.cwd}`
        : label;
      return `<span class="badge branch" title="${escapeHtml(title)}" aria-label="${escapeHtml(label)}">${branchIcon()}</span>`;
    }

    function sourceBadge(source) {
      if (source !== 'archived') return '';
      const text = t('archived');
      return `<span class="badge ${escapeHtml(source)}">${text}</span>`;
    }

    function periodParams(period = state.period) {
      const params = new URLSearchParams({ period });
      if (period === 'custom') {
        if (state.customStartDate) params.set('start', state.customStartDate);
        if (state.customEndDate) params.set('end', state.customEndDate);
      }
      return params;
    }

    function periodCacheKey(period = state.period, startDate = state.customStartDate, endDate = state.customEndDate) {
      return `${period}|${startDate || ''}|${endDate || ''}`;
    }

    async function applySessionData(data, requestedPeriod, requestedStart, requestedEnd, selectTop = true) {
      state.sessions = data.sessions || [];
      state.summary = data.summary || null;
      state.dailyUsage = data.daily_usage || [];
      state.remotes = data.remotes || [];
      state.currentDeviceShortCode = data.current_device_short_code || '';
      state.codexHome = data.codex_home || '';
      state.codexSources = data.codex_sources || [];
      state.generatedAt = data.generated_at || '';
      state.period = data.period?.key || requestedPeriod;
      if (state.period === 'custom') {
        state.customStartDate = data.period?.start_date || requestedStart;
        state.customEndDate = data.period?.end_date || requestedEnd || state.customStartDate;
      }
      document.getElementById('codexHome').textContent = `${state.codexHome || ''} · ${fmtDate(state.generatedAt)} ${t('scanned')}`;
      updateRemoteButton();
      populateEnvironmentFilter();
      populateModelFilter();
      if (state.selectedUid && !state.sessions.some(row => row.uid === state.selectedUid)) {
        state.selectedUid = null;
      }
      renderAll();
      const visibleRows = currentSelectableRows();
      if ((selectTop || !state.selectedUid) && visibleRows.length) {
        const first = visibleRows[0];
        if (first) await showDetails(first.uid);
      } else if (state.selectedUid) {
        await showDetails(state.selectedUid, false);
      } else {
        clearDetails();
      }
    }

    async function loadData(selectTop = true, options = {}) {
      if (state.loading) {
        if (!options.silent) state.reloadAfterLoad = true;
        return;
      }
      state.loading = true;
      state.reloadAfterLoad = false;
      const requestedPeriod = state.period;
      const requestedStart = state.customStartDate;
      const requestedEnd = state.customEndDate;
      const cacheKey = periodCacheKey(requestedPeriod, requestedStart, requestedEnd);
      const refreshBtn = document.getElementById('refreshBtn');
      refreshBtn.disabled = true;
      refreshBtn.textContent = t('scanning');
      try {
        const res = await fetch('/api/sessions?' + periodParams(requestedPeriod).toString(), { cache: 'no-store' });
        if (!res.ok) throw new Error('HTTP ' + res.status);
        const data = await res.json();
        if (
          state.period !== requestedPeriod
          || state.customStartDate !== requestedStart
          || state.customEndDate !== requestedEnd
        ) {
          state.reloadAfterLoad = true;
          return;
        }
        state.periodCache.set(cacheKey, data);
        await applySessionData(data, requestedPeriod, requestedStart, requestedEnd, selectTop);
      } catch (err) {
        document.getElementById('sessionRows').innerHTML = `<tr><td colspan="8" class="error">${escapeHtml(t('loadFailed', { message: err.message }))}</td></tr>`;
      } finally {
        refreshBtn.disabled = false;
        refreshBtn.textContent = t('refresh');
        state.loading = false;
        if (state.reloadAfterLoad) {
          state.reloadAfterLoad = false;
          loadData(true);
        }
      }
    }

    function populateModelFilter() {
      const select = document.getElementById('modelFilter');
      const oldValue = select.value;
      const models = Array.from(new Set(state.sessions.map(row => row.model || 'unknown'))).sort();
      select.innerHTML = `<option value="all">${escapeHtml(t('allModels'))}</option>` + models.map(model => `<option value="${escapeHtml(model)}">${escapeHtml(model)}</option>`).join('');
      select.value = models.includes(oldValue) ? oldValue : 'all';
      state.model = select.value;
    }

    function baseFilteredSessions() {
      const needle = state.search.trim().toLowerCase();
      return state.sessions.filter(row => {
        if (state.environment !== 'all' && (row.environment_id || row.environment || 'local') !== state.environment) return false;
        if (state.source !== 'all' && row.source !== state.source) return false;
        if (state.model !== 'all' && (row.model || 'unknown') !== state.model) return false;
        if (!needle) return true;
        const haystack = [
          row.title, row.session_id, row.model, row.project, row.project_root,
          row.workspace_root, row.project_branch, row.cwd, row.path, row.source, row.environment
        ].join(' ').toLowerCase();
        return haystack.includes(needle);
      });
    }

    function sortSessions(rows) {
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
        const result = String(av).localeCompare(String(bv), locale());
        return state.sortDir === 'asc' ? result : -result;
      });
      return rows;
    }

    function filteredSessions() {
      let rows = sortSessions([...baseFilteredSessions()]);

      if (state.limit !== 'all') rows = rows.slice(0, Number(state.limit));
      return rows;
    }

    function projectGroups() {
      const groups = new Map();
      baseFilteredSessions().forEach(row => {
        const key = projectKey(row);
        if (!groups.has(key)) {
          groups.set(key, {
            key,
            label: projectName(row),
            workspace: workspaceKey(row),
            environment: row.environment || '',
            environment_id: row.environment_id || '',
            is_remote: Boolean(row.is_remote),
            rows: [],
            usage: zeroClientUsage(),
            latestTime: 0,
          });
        }
        const group = groups.get(key);
        group.rows.push(row);
        group.usage = addClientUsage(group.usage, usageOf(row));
        group.latestTime = Math.max(group.latestTime, rowTime(row));
        if (!group.environment && row.environment) group.environment = row.environment;
        if (!group.environment_id && row.environment_id) group.environment_id = row.environment_id;
        if (row.is_remote) group.is_remote = true;
      });

      return Array.from(groups.values())
        .map(group => {
          group.rows.sort((a, b) => {
            const archivedDelta = (a.source === 'archived' ? 1 : 0) - (b.source === 'archived' ? 1 : 0);
            if (archivedDelta) return archivedDelta;
            return rowTime(b) - rowTime(a) || String(a.title || '').localeCompare(String(b.title || ''), locale());
          });
          return group;
        })
        .sort((a, b) => b.latestTime - a.latestTime || a.label.localeCompare(b.label, locale()));
    }

    function isProjectExpanded(group) {
      return state.projectExpanded[group.key] !== false;
    }

    function isProjectShowingAll(group) {
      return state.projectShowAll[group.key] === true;
    }

    function visibleProjectRows() {
      return projectGroups().flatMap(group => {
        if (!isProjectExpanded(group)) return [];
        return isProjectShowingAll(group) ? group.rows : group.rows.slice(0, projectPreviewLimit);
      });
    }

    function currentSelectableRows() {
      return state.viewMode === 'project' ? visibleProjectRows() : filteredSessions();
    }

    function renderAll() {
      renderMetrics();
      updatePeriodButtons();
      updateViewButtons();
      document.getElementById('limitSelect').disabled = state.viewMode === 'project';
      renderTable();
      if (state.calendarOpen) renderCalendar();
    }

    function setPeriod(period) {
      if (state.period === period) return;
      state.period = period;
      state.selectedUid = null;
      state.projectExpanded = {};
      state.projectShowAll = {};
      updatePeriodButtons();
      const cached = state.periodCache.get(periodCacheKey());
      if (cached) {
        applySessionData(cached, state.period, state.customStartDate, state.customEndDate, false);
      } else {
        renderMetrics();
        clearDetails();
      }
      loadData(false);
    }

    function setCustomPeriod(startDate, endDate) {
      state.period = 'custom';
      state.customStartDate = startDate;
      state.customEndDate = endDate || startDate;
      state.selectedUid = null;
      state.projectExpanded = {};
      state.projectShowAll = {};
      state.calendarOpen = false;
      updateCalendarVisibility();
      updatePeriodButtons();
      const cached = state.periodCache.get(periodCacheKey());
      if (cached) {
        applySessionData(cached, state.period, state.customStartDate, state.customEndDate, false);
      } else {
        clearDetails();
      }
      loadData(false);
    }

    function setPrimaryView(mode) {
      state.viewMode = mode === 'recent' || mode === 'total' ? mode : 'project';
      if (state.viewMode === 'recent') state.sortKey = 'end_at';
      if (state.viewMode === 'total') state.sortKey = 'total_tokens';
      state.sortDir = 'desc';
      renderAll();
    }

    function updatePeriodButtons() {
      document.querySelectorAll('[data-period-button]').forEach(button => {
        button.classList.toggle('active', button.dataset.periodButton === state.period);
      });
      document.getElementById('calendarBtn').classList.toggle('active', state.period === 'custom');
    }

    function updateViewButtons() {
      document.querySelectorAll('[data-view-button]').forEach(button => {
        button.classList.toggle('active', button.dataset.viewButton === state.viewMode);
      });
    }

    function periodLabel() {
      const keys = {
        today: 'periodToday',
        '7d': 'period7d',
        '30d': 'period30d',
        week: 'periodWeek',
        month: 'periodMonth',
        all: 'periodAll',
      };
      if (state.period === 'custom') {
        return t('periodCustom', {
          start: shortDateLabel(state.customStartDate),
          end: shortDateLabel(state.customEndDate || state.customStartDate),
        });
      }
      return t(keys[state.period] || 'periodToday');
    }

    function dateFromKey(key) {
      const parts = String(key || '').split('-').map(Number);
      if (parts.length !== 3 || parts.some(part => !Number.isFinite(part))) return null;
      return new Date(parts[0], parts[1] - 1, parts[2]);
    }

    function dateKey(date) {
      const year = date.getFullYear();
      const month = String(date.getMonth() + 1).padStart(2, '0');
      const day = String(date.getDate()).padStart(2, '0');
      return `${year}-${month}-${day}`;
    }

    function shortDateLabel(key) {
      const date = dateFromKey(key);
      if (!date) return '';
      return `${String(date.getMonth() + 1).padStart(2, '0')}/${String(date.getDate()).padStart(2, '0')}`;
    }

    function monthKey(date) {
      return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}`;
    }

    function usageByDate() {
      const map = new Map();
      state.dailyUsage.forEach(row => {
        map.set(row.date, Number(row.usage?.total_tokens || 0));
      });
      return map;
    }

    function latestUsageDate() {
      const dates = state.dailyUsage.map(row => row.date).filter(Boolean).sort();
      return dates.length ? dates[dates.length - 1] : dateKey(new Date());
    }

    function openCalendar() {
      state.calendarOpen = true;
      state.calendarDraftStart = state.customStartDate || '';
      state.calendarDraftEnd = state.customEndDate || '';
      const seed = state.calendarDraftStart || latestUsageDate();
      state.calendarMonth = monthKey(dateFromKey(seed) || new Date());
      updateCalendarVisibility();
      renderCalendar();
    }

    function closeCalendar() {
      state.calendarOpen = false;
      updateCalendarVisibility();
    }

    function updateCalendarVisibility() {
      const popover = document.getElementById('calendarPopover');
      popover.hidden = !state.calendarOpen;
    }

    function changeCalendarMonth(delta) {
      const current = dateFromKey(`${state.calendarMonth || monthKey(new Date())}-01`) || new Date();
      current.setMonth(current.getMonth() + delta);
      state.calendarMonth = monthKey(current);
      renderCalendar();
    }

    function inDraftRange(key) {
      if (!state.calendarDraftStart) return false;
      const start = state.calendarDraftStart;
      const end = state.calendarDraftEnd || state.calendarDraftStart;
      return key >= start && key <= end;
    }

    function isDraftEdge(key) {
      return key === state.calendarDraftStart || key === state.calendarDraftEnd;
    }

    function selectCalendarDate(key) {
      if (!state.calendarDraftStart || state.calendarDraftEnd) {
        state.calendarDraftStart = key;
        state.calendarDraftEnd = '';
        renderCalendar();
        return;
      }
      const start = state.calendarDraftStart;
      if (key < start) setCustomPeriod(key, start);
      else setCustomPeriod(start, key);
    }

    function applyCalendarRange() {
      if (!state.calendarDraftStart) return;
      setCustomPeriod(state.calendarDraftStart, state.calendarDraftEnd || state.calendarDraftStart);
    }

    function renderCalendar() {
      const popover = document.getElementById('calendarPopover');
      if (!popover || !state.calendarOpen) return;

      const usageMap = usageByDate();
      const todayKey = dateKey(new Date());
      const monthStart = dateFromKey(`${state.calendarMonth || monthKey(new Date())}-01`) || new Date();
      monthStart.setDate(1);
      const title = monthStart.toLocaleDateString(locale(), { year: 'numeric', month: 'long' });
      const firstGridDate = new Date(monthStart);
      firstGridDate.setDate(monthStart.getDate() - ((monthStart.getDay() + 6) % 7));
      const weekdays = (I18N[state.lang] && I18N[state.lang].calendarWeekdays) || I18N.zh.calendarWeekdays;
      const nextMonth = new Date(monthStart);
      nextMonth.setMonth(nextMonth.getMonth() + 1);
      const nextDisabled = monthKey(nextMonth) > monthKey(new Date()) ? 'disabled' : '';

      const days = [];
      for (let index = 0; index < 42; index += 1) {
        const date = new Date(firstGridDate);
        date.setDate(firstGridDate.getDate() + index);
        const key = dateKey(date);
        const tokens = usageMap.get(key) || 0;
        const classes = [
          'calendar-day',
          date.getMonth() === monthStart.getMonth() ? '' : 'outside',
          inDraftRange(key) ? 'in-range' : '',
          isDraftEdge(key) ? 'range-edge' : '',
        ].filter(Boolean).join(' ');
        const disabled = key > todayKey ? 'disabled' : '';
        days.push(`
          <button class="${classes}" type="button" data-calendar-date="${key}" ${disabled}>
            <span>${date.getDate()}</span>
            ${tokens ? `<span class="calendar-usage">${escapeHtml(fmtCompact(tokens))}</span>` : ''}
          </button>
        `);
      }

      popover.innerHTML = `
        <div class="calendar-head">
          <button type="button" title="${escapeHtml(t('calendarPrev'))}" data-calendar-prev>&lt;</button>
          <div class="calendar-title">${escapeHtml(title)}</div>
          <button type="button" title="${escapeHtml(t('calendarNext'))}" data-calendar-next ${nextDisabled}>&gt;</button>
        </div>
        <div class="calendar-grid">
          ${weekdays.map(day => `<div class="calendar-weekday">${escapeHtml(day)}</div>`).join('')}
          ${days.join('')}
        </div>
        <div class="calendar-actions">
          <button type="button" data-calendar-cancel>${escapeHtml(t('calendarCancel'))}</button>
          <button class="primary" type="button" data-calendar-apply ${state.calendarDraftStart ? '' : 'disabled'}>${escapeHtml(t('calendarApply'))}</button>
        </div>
      `;

      popover.querySelector('[data-calendar-prev]').addEventListener('click', () => changeCalendarMonth(-1));
      popover.querySelector('[data-calendar-next]').addEventListener('click', () => changeCalendarMonth(1));
      popover.querySelector('[data-calendar-cancel]').addEventListener('click', closeCalendar);
      popover.querySelector('[data-calendar-apply]').addEventListener('click', applyCalendarRange);
      popover.querySelectorAll('[data-calendar-date]').forEach(button => {
        button.addEventListener('click', () => selectCalendarDate(button.dataset.calendarDate));
      });
    }

    function renderMetrics() {
      const usage = state.summary && state.summary.usage ? state.summary.usage : {};
      const environmentRows = state.summary?.by_environment || [];
      const environmentHint = environmentRows.length > 1
        ? environmentRows.map(row => `${row.label} ${fmt(row.sessions)}`).join(' · ')
        : '';
      const sessionHint = environmentHint
        ? t('metricSessionsHintWithEnvs', { active: fmt(state.summary?.active_count || 0), archived: fmt(state.summary?.archived_count || 0), envs: environmentHint })
        : t('metricSessionsHint', { active: fmt(state.summary?.active_count || 0), archived: fmt(state.summary?.archived_count || 0) });
      const metrics = [
        [t('metricSessions'), fmt(state.summary?.session_count || 0), sessionHint],
        [t('metricPeriodTotalTokens', { period: periodLabel() }), fmtCompact(usage.total_tokens), fmt(usage.total_tokens)],
        [t('metricCost'), fmtUsd(state.summary?.estimated_cost_usd), t('metricCostHint', { count: fmt(state.summary?.estimated_cost_known_count || 0) })],
        [t('metricInput'), fmtCompact(usage.input_tokens), fmt(usage.input_tokens)],
        [t('metricCached'), fmtCompact(usage.cached_input_tokens), fmt(usage.cached_input_tokens)],
        [t('metricOutput'), fmtCompact(usage.output_tokens), fmt(usage.output_tokens)],
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
      if (state.viewMode === 'project') {
        renderProjectTable();
        return;
      }

      const rows = filteredSessions();
      document.getElementById('resultCount').textContent = t('rowCount', { count: fmt(rows.length) });
      if (!rows.length) {
        document.getElementById('sessionRows').innerHTML = `<tr><td colspan="8" class="empty">${escapeHtml(t('noMatches'))}</td></tr>`;
        return;
      }
      document.getElementById('sessionRows').innerHTML = rows.map(row => sessionRowHtml(row)).join('');
      bindTableInteractions();
    }

    function sessionRowHtml(row, extraClass = '', options = {}) {
      const usage = usageOf(row);
      const selected = row.uid === state.selectedUid ? 'selected' : '';
      const classes = [selected, extraClass].filter(Boolean).join(' ');
      const timeValue = row.end_at || row.updated_at || row.start_at;
      const compactProject = options.compactProject === true;
      return `
        <tr class="${escapeHtml(classes)}" data-uid="${escapeHtml(row.uid)}">
          <td class="title-cell" title="${escapeHtml((row.title || row.session_id) + (timeValue ? ' · ' + fmtDate(timeValue) : ''))}">
            <div class="title-line">
              ${compactProject ? sourceBadge(row.source) : ''}
              ${compactProject ? branchBadge(row) : ''}
              <div class="title-main">${escapeHtml(row.title || row.session_id)}</div>
              <div class="title-time">${escapeHtml(fmtRelativeTime(timeValue))}</div>
            </div>
            ${compactProject ? '' : `<div class="title-sub">${environmentBadge(row)} ${sourceBadge(row.source)} ${branchBadge(row)} <span class="title-sub-text">${escapeHtml(row.project || shortPath(row.cwd))}</span></div>`}
          </td>
          <td class="number" title="${fmt(usage.total_tokens)} tokens"><strong>${fmtCompact(usage.total_tokens)}</strong></td>
          <td class="number" title="${fmt(usage.output_tokens)} output tokens">${fmtCompact(usage.output_tokens)}</td>
          <td class="number" title="${escapeHtml(row.price_model_known ? t('priceKnown') : t('priceUnknown'))}">${fmtUsd(row.estimated_cost_usd)}</td>
          <td class="number">${fmtPercent(row.cached_input_percent)}</td>
          <td class="number">${fmt(row.turn_count)}</td>
          <td class="model-cell" title="${escapeHtml(row.model || 'unknown')}">${escapeHtml(row.model || 'unknown')}</td>
          <td class="effort-cell" title="${escapeHtml(row.effort || 'N/A')}">${escapeHtml(row.effort || 'N/A')}</td>
        </tr>
      `;
    }

    function projectHeaderHtml(group) {
      const expanded = isProjectExpanded(group);
      const latest = group.rows[0]?.end_at || group.rows[0]?.updated_at || group.rows[0]?.start_at || '';
      return `
        <tr class="project-group-row">
          <td class="title-cell">
            <div class="project-title-cell">
              <button class="project-toggle" type="button" data-project-toggle="${escapeHtml(group.key)}" title="${escapeHtml(t(expanded ? 'collapseProject' : 'expandProject'))}">
                ${folderIcon()}
                <span class="project-name" title="${escapeHtml(group.label)}">${escapeHtml(group.label)}</span>
              </button>
              ${environmentBadge(group)}
            </div>
          </td>
          <td class="number" title="${fmt(group.usage.total_tokens)} tokens"><strong>${escapeHtml(fmtCompact(group.usage.total_tokens))}</strong></td>
          <td class="project-summary-cell" colspan="6">
            <div class="project-meta">
              <span>${escapeHtml(t('projectConversationCount', { count: fmt(group.rows.length) }))}</span>
              <span>${escapeHtml(t('projectLatest', { time: fmtRelativeTime(latest) }))}</span>
            </div>
          </td>
        </tr>
      `;
    }

    function projectMoreRowHtml(group, hiddenCount) {
      const showingAll = isProjectShowingAll(group);
      const label = showingAll ? t('showFewerConversations') : t('showMoreConversations', { count: fmt(hiddenCount) });
      return `
        <tr class="project-more-row">
          <td colspan="8">
            <button class="project-more-btn" type="button" data-project-more="${escapeHtml(group.key)}" data-project-more-state="${showingAll ? 'less' : 'more'}">${escapeHtml(label)}</button>
          </td>
        </tr>
      `;
    }

    function renderProjectTable() {
      const groups = projectGroups();
      const sessionCount = groups.reduce((sum, group) => sum + group.rows.length, 0);
      document.getElementById('resultCount').textContent = t('projectRowCount', { projects: fmt(groups.length), sessions: fmt(sessionCount) });
      if (!groups.length) {
        document.getElementById('sessionRows').innerHTML = `<tr><td colspan="8" class="empty">${escapeHtml(t('noMatches'))}</td></tr>`;
        return;
      }

      document.getElementById('sessionRows').innerHTML = groups.map(group => {
        const expanded = isProjectExpanded(group);
        const showingAll = isProjectShowingAll(group);
        const visibleRows = expanded
          ? (showingAll ? group.rows : group.rows.slice(0, projectPreviewLimit))
          : [];
        const hiddenCount = Math.max(0, group.rows.length - projectPreviewLimit);
        return [
          projectHeaderHtml(group),
          ...visibleRows.map(row => sessionRowHtml(row, 'project-session-row', { compactProject: true })),
          expanded && hiddenCount > 0 ? projectMoreRowHtml(group, hiddenCount) : '',
        ].join('');
      }).join('');
      bindTableInteractions();
    }

    function bindTableInteractions() {
      document.querySelectorAll('#sessionRows tr[data-uid]').forEach(row => {
        row.addEventListener('click', () => showDetails(row.dataset.uid));
      });
      document.querySelectorAll('[data-project-toggle]').forEach(button => {
        button.addEventListener('click', event => {
          event.stopPropagation();
          const key = button.dataset.projectToggle;
          state.projectExpanded[key] = state.projectExpanded[key] === false;
          renderAll();
        });
      });
      document.querySelectorAll('[data-project-more]').forEach(button => {
        button.addEventListener('click', event => {
          event.stopPropagation();
          const key = button.dataset.projectMore;
          state.projectShowAll[key] = button.dataset.projectMoreState === 'more';
          renderAll();
        });
      });
    }

    function clearDetails() {
      document.getElementById('detailStatus').textContent = t('notSelected');
      document.getElementById('detailsBody').innerHTML = `<div class="empty">${escapeHtml(t('selectRow'))}</div>`;
    }

    async function showDetails(uid, renderRows = true) {
      state.selectedUid = uid;
      if (renderRows) renderTable();
      document.getElementById('detailStatus').textContent = t('detailsLoading');
      try {
        const params = periodParams();
        params.set('id', uid);
        const res = await fetch('/api/session?' + params.toString(), { cache: 'no-store' });
        if (!res.ok) throw new Error('HTTP ' + res.status);
        const detail = await res.json();
        renderDetails(detail);
      } catch (err) {
        document.getElementById('detailsBody').innerHTML = `<div class="error">${escapeHtml(t('detailFailed', { message: err.message }))}</div>`;
        document.getElementById('detailStatus').textContent = t('failed');
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

      document.getElementById('detailStatus').textContent = t('countEvents', { count: fmt(detail.token_event_count) });
      document.getElementById('detailsBody').innerHTML = `
        <div class="detail-title">${escapeHtml(detail.title || detail.session_id)}</div>
        <div>${environmentBadge(detail)} ${sourceBadge(detail.source)} <span class="badge">${escapeHtml(detail.model || 'unknown')}</span> <span class="badge">${escapeHtml(t('turnSuffix', { count: fmt(detail.turn_count) }))}</span></div>

        <div class="breakdown">
          ${breakdownRow(t('input'), 'input', uncachedInputTokens, max, costs.input_tokens)}
          ${breakdownRow(t('cached'), 'cached', usage.cached_input_tokens, max, costs.cached_input_tokens)}
          ${breakdownRow(t('output'), 'output', usage.output_tokens, max, costs.output_tokens)}
          ${breakdownRow(t('reasoning'), 'reasoning', usage.reasoning_output_tokens, max, costs.reasoning_output_tokens, t('reasoningCostTitle'))}
        </div>

        <div class="section-label">${escapeHtml(t('cumulativeChart'))}</div>
        <canvas id="timelineCanvas" width="720" height="220"></canvas>

        <div class="section-label">${escapeHtml(t('metadata'))}</div>
        <div class="detail-meta">
          ${kv(t('totalTokens'), fmt(usage.total_tokens))}
          ${kv(t('cost'), fmtUsd(detail.estimated_cost_usd))}
          ${kv(t('cachePercent'), detail.cached_input_percent == null ? '' : detail.cached_input_percent + '%')}
          ${kv(t('reasoningEffort'), detail.effort || 'N/A')}
          ${kv(t('time'), `${fmtRelativeTime(detail.end_at)} (${fmtDate(detail.start_at)} - ${fmtDate(detail.end_at)})`)}
          ${kv(t('totalDuration'), fmtDuration(detail.duration_ms_total))}
          ${kv(t('ttftAvg'), fmtDuration(detail.time_to_first_token_ms_avg))}
          ${kv(t('environment'), detail.environment || '')}
          ${kv(t('project'), detail.project || '')}
          ${kv(t('projectRoot'), detail.project_root || '', 'path')}
          ${detail.is_git_worktree ? kv(t('workspaceRoot'), detail.workspace_root || '', 'path') : ''}
          ${detail.project_branch ? kv(t('branch'), detail.project_branch || '') : ''}
          ${kv(t('cwd'), detail.cwd || '', 'path')}
          ${kv(t('codexHome'), detail.codex_home || '', 'path')}
          ${kv(t('logFile'), detail.path || '', 'path')}
          ${kv('Session ID', detail.session_id || '', 'path')}
        </div>

        ${detail.first_user_prompt ? `<div class="section-label">${escapeHtml(t('firstUserPrompt'))}</div><div class="notice">${escapeHtml(detail.first_user_prompt)}</div>` : ''}
        ${detail.last_agent_preview ? `<div class="section-label">${escapeHtml(t('lastReplySummary'))}</div><div class="notice">${escapeHtml(detail.last_agent_preview)}</div>` : ''}

        <div class="section-label">${escapeHtml(t('toolCalls'))}</div>
        ${toolRows ? `<table class="mini-table"><thead><tr><th>${escapeHtml(t('tool'))}</th><th>${escapeHtml(t('count'))}</th></tr></thead><tbody>${toolRows}</tbody></table>` : `<div class="notice">${escapeHtml(t('noToolCalls'))}</div>`}

        <div class="section-label">${escapeHtml(t('timelineDetails'))}</div>
        ${timelineRows ? `<div class="table-wrap" style="max-height:260px"><table class="mini-table"><thead><tr><th>${escapeHtml(t('timelineTime'))}</th><th>${escapeHtml(t('timelineTotal'))}</th><th>${escapeHtml(t('input'))}</th><th>${escapeHtml(t('cached'))}</th><th>${escapeHtml(t('output'))}</th><th>${escapeHtml(t('reasoning'))}</th></tr></thead><tbody>${timelineRows}</tbody></table></div>` : `<div class="notice">${escapeHtml(t('noTimeline'))}</div>`}
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
        ctx.fillText(t('noCurve'), 14, 24);
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
      ctx.fillText(t('countPoints', { count: points.length }), 14, height - 8);
    }

    function updateRemoteButton() {
      const button = document.getElementById('remoteBtn');
      if (!button) return;
      button.textContent = state.remotes.length ? t('manageRemote') : t('importRemote');
      button.title = t('importRemoteTitle');
    }

    function openRemoteModal() {
      renderRemoteModal();
      document.getElementById('remoteModal').hidden = false;
    }

    function closeRemoteModal() {
      document.getElementById('remoteModal').hidden = true;
      state.pendingRemoteSnapshot = null;
    }

    function remoteStatus(message = '', isError = false) {
      const el = document.getElementById('remoteStatus');
      if (!el) return;
      el.textContent = message;
      el.classList.toggle('error-text', isError);
    }

    function renderRemoteModal(status = '') {
      const body = document.getElementById('remoteModalBody');
      const rows = state.remotes || [];
      const table = rows.length ? `
        <table class="remote-table">
          <thead>
            <tr>
              <th>${escapeHtml(t('remoteName'))}</th>
              <th>${escapeHtml(t('remoteCode'))}</th>
              <th>${escapeHtml(t('remoteSessions'))}</th>
              <th>${escapeHtml(t('remoteUpdated'))}</th>
              <th>${escapeHtml(t('remoteImportedAt'))}</th>
              <th>${escapeHtml(t('remoteActions'))}</th>
            </tr>
          </thead>
          <tbody>
            ${rows.map(row => `
              <tr>
                <td><strong>${escapeHtml(row.label || row.device_short_code)}</strong></td>
                <td class="path remote-code">${escapeHtml(row.device_short_code || '')}</td>
                <td class="remote-count">${fmt(row.session_count || 0)}</td>
                <td class="remote-date">${escapeHtml(fmtDate(row.generated_at || row.exported_at))}</td>
                <td class="remote-date">${escapeHtml(fmtDate(row.imported_at))}</td>
                <td class="remote-action-cell">
                  <div class="remote-actions">
                    <button type="button" data-remote-update="${escapeHtml(row.device_short_code)}">${escapeHtml(t('remoteUpdate'))}</button>
                    <button type="button" data-remote-rename="${escapeHtml(row.device_short_code)}">${escapeHtml(t('remoteRename'))}</button>
                    <button class="danger" type="button" data-remote-delete="${escapeHtml(row.device_short_code)}">${escapeHtml(t('remoteDelete'))}</button>
                  </div>
                </td>
              </tr>
            `).join('')}
          </tbody>
        </table>
      ` : `<div class="empty">${escapeHtml(t('remoteEmpty'))}</div>`;

      body.innerHTML = `
        <div class="status-line">${escapeHtml(t('remoteImportHelp'))}</div>
        ${table}
        <div class="status-line" id="remoteStatus">${escapeHtml(status)}</div>
      `;
      bindRemoteRows();
      updateRemoteButton();
    }

    function bindRemoteRows() {
      document.querySelectorAll('[data-remote-update]').forEach(button => {
        button.addEventListener('click', () => chooseRemoteFile());
      });
      document.querySelectorAll('[data-remote-rename]').forEach(button => {
        button.addEventListener('click', () => renameRemote(button.dataset.remoteRename));
      });
      document.querySelectorAll('[data-remote-delete]').forEach(button => {
        button.addEventListener('click', () => deleteRemote(button.dataset.remoteDelete));
      });
    }

    function chooseRemoteFile() {
      const input = document.getElementById('remoteFileInput');
      input.value = '';
      input.click();
    }

    async function readJsonFile(file) {
      const text = await file.text();
      return JSON.parse(text);
    }

    async function importRemoteSnapshot(snapshot, options = {}) {
      const res = await fetch('/api/remotes/import', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ snapshot, ...options }),
      });
      const data = await res.json();
      if (res.ok && data.ok) {
        state.periodCache.clear();
        await loadData(false);
        renderRemoteModal(t('remoteImported'));
        document.getElementById('remoteModal').hidden = false;
        return;
      }
      if (data.needs_label) {
        const label = window.prompt(t('remoteNeedLabel'), data.suggested_label || '');
        if (!label) {
          remoteStatus('', false);
          return;
        }
        await importRemoteSnapshot(snapshot, { label });
        return;
      }
      if (data.needs_confirmation && data.reason === 'current_device') {
        if (window.confirm(t('remoteCurrentWarning'))) {
          const label = window.prompt(t('remoteNeedLabel'), data.suggested_label || '');
          await importRemoteSnapshot(snapshot, { allow_current_device: true, label: label || data.suggested_label || '' });
        }
        return;
      }
      throw new Error(data.error || data.reason || 'unknown');
    }

    async function handleRemoteFile(file) {
      if (!file) return;
      try {
        remoteStatus(t('loading'));
        const snapshot = await readJsonFile(file);
        await importRemoteSnapshot(snapshot);
      } catch (err) {
        remoteStatus(t('remoteImportFailed', { message: err.message }), true);
      }
    }

    async function renameRemote(deviceCode) {
      const row = state.remotes.find(item => item.device_short_code === deviceCode);
      const label = window.prompt(t('remoteName'), row?.label || deviceCode);
      if (!label) return;
      try {
        const res = await fetch('/api/remotes/rename', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ device_short_code: deviceCode, label }),
        });
        const data = await res.json();
        if (!res.ok || !data.ok) throw new Error(data.error || 'unknown');
        state.periodCache.clear();
        await loadData(false);
        renderRemoteModal(t('remoteRenamed'));
      } catch (err) {
        remoteStatus(t('remoteImportFailed', { message: err.message }), true);
      }
    }

    async function deleteRemote(deviceCode) {
      if (!window.confirm(t('remoteDeleteConfirm'))) return;
      try {
        const res = await fetch('/api/remotes/delete', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ device_short_code: deviceCode, confirm: true }),
        });
        const data = await res.json();
        if (!res.ok || !data.ok) throw new Error(data.error || 'unknown');
        state.periodCache.clear();
        await loadData(false);
        renderRemoteModal(t('remoteDeleted'));
      } catch (err) {
        remoteStatus(t('remoteImportFailed', { message: err.message }), true);
      }
    }

    document.getElementById('refreshBtn').addEventListener('click', () => loadData(false));
    document.getElementById('remoteBtn').addEventListener('click', () => {
      if (state.remotes.length) openRemoteModal();
      else chooseRemoteFile();
    });
    document.getElementById('remoteImportBtn').addEventListener('click', chooseRemoteFile);
    document.getElementById('remoteCloseBtn').addEventListener('click', closeRemoteModal);
    document.getElementById('remoteModal').addEventListener('click', event => {
      if (event.target.id === 'remoteModal') closeRemoteModal();
    });
    document.getElementById('remoteFileInput').addEventListener('change', event => {
      handleRemoteFile(event.target.files && event.target.files[0]);
    });
    document.getElementById('snapshotExportBtn').addEventListener('click', () => { window.location.href = '/api/export.json'; });
    document.getElementById('searchInput').addEventListener('input', event => { state.search = event.target.value; renderAll(); });
    document.getElementById('environmentFilter').addEventListener('change', event => { state.environment = event.target.value; renderAll(); });
    document.getElementById('sourceFilter').addEventListener('change', event => { state.source = event.target.value; renderAll(); });
    document.getElementById('modelFilter').addEventListener('change', event => { state.model = event.target.value; renderAll(); });
    document.querySelectorAll('[data-lang-button]').forEach(button => {
      button.addEventListener('click', () => setLanguage(button.dataset.langButton));
    });
    document.querySelectorAll('[data-period-button]').forEach(button => {
      button.addEventListener('click', () => setPeriod(button.dataset.periodButton));
    });
    document.getElementById('periodWrap').addEventListener('click', event => {
      event.stopPropagation();
    });
    document.getElementById('calendarBtn').addEventListener('click', event => {
      event.stopPropagation();
      if (state.calendarOpen) closeCalendar();
      else openCalendar();
    });
    document.querySelectorAll('[data-view-button]').forEach(button => {
      button.addEventListener('click', () => setPrimaryView(button.dataset.viewButton));
    });
    document.getElementById('limitSelect').addEventListener('change', event => { state.limit = event.target.value; renderAll(); });
    document.querySelectorAll('th[data-sort]').forEach(th => {
      th.addEventListener('click', () => {
        const key = th.dataset.sort;
        state.viewMode = key === 'total_tokens' ? 'total' : 'recent';
        if (state.sortKey === key) state.sortDir = state.sortDir === 'desc' ? 'asc' : 'desc';
        else { state.sortKey = key; state.sortDir = 'desc'; }
        renderAll();
      });
    });
    window.addEventListener('resize', () => {
      if (state.selectedUid) showDetails(state.selectedUid, false);
    });
    document.addEventListener('click', event => {
      if (state.calendarOpen && !event.target.closest('#periodWrap')) closeCalendar();
    });
    document.addEventListener('keydown', event => {
      if (event.key === 'Escape' && state.calendarOpen) closeCalendar();
    });

    applyStaticText();
    populateEnvironmentFilter();
    populateSourceFilter();
    populateLimitSelect();
    loadData(true);
    setInterval(() => loadData(false, { silent: true }), 10000);
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
            query = parse_qs(parsed.query)
            if path == "/":
                self.send_bytes(HTML.encode("utf-8"), "text/html; charset=utf-8")
                return

            if path == "/api/health":
                self.send_json(
                    {
                        "ok": True,
                        "app": "codex-usage-dashboard",
                        "features": DASHBOARD_FEATURES,
                        "device_short_code": analyzer.remote_store.current_device_code if analyzer.remote_store else current_device_short_code(),
                    }
                )
                return

            if path == "/api/sessions":
                period = query.get("period", ["today"])[0]
                start_date = query.get("start", [""])[0] or None
                end_date = query.get("end", [""])[0] or None
                snapshot = analyzer.scan(period, start_date, end_date)
                payload = {
                    "generated_at": snapshot["generated_at"],
                    "codex_home": snapshot["codex_home"],
                    "codex_sources": snapshot.get("codex_sources", []),
                    "period": snapshot["period"],
                    "summary": snapshot["summary"],
                    "sessions": snapshot["sessions"],
                    "daily_usage": snapshot.get("daily_usage", []),
                    "current_device_short_code": analyzer.remote_store.current_device_code if analyzer.remote_store else current_device_short_code(),
                    "remotes": analyzer.remote_store.list_remotes() if analyzer.remote_store else [],
                }
                self.send_json(payload)
                return

            if path == "/api/session":
                uid = query.get("id", [""])[0]
                period = query.get("period", ["today"])[0]
                start_date = query.get("start", [""])[0] or None
                end_date = query.get("end", [""])[0] or None
                detail = analyzer.get_detail(uid, period, start_date, end_date)
                if detail is None:
                    self.send_json({"error": "session not found"}, status=404)
                    return
                self.send_json(detail)
                return

            if path == "/api/export.json":
                body = analyzer.export_snapshot_json().encode("utf-8")
                device_code = analyzer.remote_store.current_device_code if analyzer.remote_store else current_device_short_code()
                filename = f"cousash-{device_code}.json"
                self.send_bytes(
                    body,
                    "application/json; charset=utf-8",
                    extra_headers={"Content-Disposition": f'attachment; filename="{filename}"'},
                )
                return

            if path == "/api/remotes":
                store = analyzer.remote_store
                self.send_json(
                    {
                        "current_device_short_code": store.current_device_code if store else current_device_short_code(),
                        "remotes": store.list_remotes() if store else [],
                    }
                )
                return

            self.send_json({"error": "not found"}, status=404)

        def do_POST(self) -> None:
            parsed = urlparse(self.path)
            path = parsed.path
            try:
                payload = self.read_json_body()
            except ValueError as exc:
                self.send_json({"ok": False, "error": str(exc)}, status=400)
                return

            store = analyzer.remote_store
            if store is None:
                self.send_json({"ok": False, "error": "remote snapshots are not enabled"}, status=400)
                return

            try:
                if path == "/api/remotes/import":
                    snapshot = payload.get("snapshot")
                    label = payload.get("label") if isinstance(payload.get("label"), str) else None
                    allow_current = bool(payload.get("allow_current_device"))
                    result = store.import_snapshot(snapshot, label=label, allow_current_device=allow_current)
                    self.send_json(result, status=200 if result.get("ok") else 409)
                    return

                if path == "/api/remotes/rename":
                    device_code = str(payload.get("device_short_code") or "")
                    label = str(payload.get("label") or "")
                    remote = store.rename_remote(device_code, label)
                    self.send_json({"ok": True, "remote": remote})
                    return

                if path == "/api/remotes/delete":
                    if payload.get("confirm") is not True:
                        self.send_json({"ok": False, "error": "delete requires confirmation"}, status=400)
                        return
                    store.delete_remote(str(payload.get("device_short_code") or ""))
                    self.send_json({"ok": True})
                    return
            except FileNotFoundError:
                self.send_json({"ok": False, "error": "remote data not found"}, status=404)
                return
            except ValueError as exc:
                self.send_json({"ok": False, "error": str(exc)}, status=400)
                return

            self.send_json({"ok": False, "error": "not found"}, status=404)

        def read_json_body(self) -> dict[str, Any]:
            length = int(self.headers.get("Content-Length") or "0")
            if length <= 0:
                return {}
            if length > 100 * 1024 * 1024:
                raise ValueError("request body is too large")
            raw = self.rfile.read(length)
            try:
                payload = json.loads(raw.decode("utf-8-sig"))
            except (UnicodeDecodeError, json.JSONDecodeError) as exc:
                raise ValueError("invalid JSON body") from exc
            if not isinstance(payload, dict):
                raise ValueError("JSON body must be an object")
            return payload

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
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass

    return Handler


class FixedPortHTTPServer(ThreadingHTTPServer):
    allow_reuse_address = True


DASHBOARD_PROCESS_MARKERS = ("codex_usage_dashboard.py", "codex-usage-dashboard")


def is_dashboard_command(command: str) -> bool:
    lowered = command.lower()
    return any(marker in lowered for marker in DASHBOARD_PROCESS_MARKERS)


def run_text(command: list[str]) -> str:
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=1.5, check=False)
    except (OSError, subprocess.SubprocessError):
        return ""
    return result.stdout.strip()


def listening_pids_for_port(port: int) -> list[int]:
    if platform.system() == "Windows":
        output = run_text(["netstat", "-ano", "-p", "TCP"])
        pids: set[int] = set()
        suffix = f":{port}"
        for line in output.splitlines():
            parts = line.split()
            if len(parts) >= 5 and parts[0].upper() == "TCP" and parts[1].endswith(suffix) and parts[3].upper() == "LISTENING":
                try:
                    pids.add(int(parts[4]))
                except ValueError:
                    pass
        return sorted(pids)

    lsof = shutil.which("lsof")
    if not lsof:
        return []
    output = run_text([lsof, "-nP", f"-iTCP:{port}", "-sTCP:LISTEN", "-t"])
    pids: list[int] = []
    for line in output.splitlines():
        try:
            pid = int(line.strip())
        except ValueError:
            continue
        if pid not in pids:
            pids.append(pid)
    return pids


def command_for_pid(pid: int) -> str:
    if pid == os.getpid():
        return ""
    if platform.system() == "Windows":
        output = run_text(["wmic", "process", "where", f"processid={pid}", "get", "CommandLine", "/value"])
        for line in output.splitlines():
            if line.startswith("CommandLine="):
                return line.removeprefix("CommandLine=").strip()
        return ""
    return run_text(["ps", "-p", str(pid), "-o", "command="])


def terminate_process(pid: int) -> None:
    if pid == os.getpid():
        return
    try:
        if platform.system() == "Windows":
            subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"], capture_output=True, timeout=3, check=False)
            return
        os.kill(pid, signal.SIGTERM)
        deadline = time.monotonic() + 2.0
        while time.monotonic() < deadline:
            try:
                os.kill(pid, 0)
            except OSError:
                return
            time.sleep(0.05)
        os.kill(pid, signal.SIGKILL)
    except (OSError, subprocess.SubprocessError):
        return


def release_dashboard_port(port: int) -> None:
    pids = listening_pids_for_port(port)
    if not pids:
        return

    blockers: list[str] = []
    for pid in pids:
        command = command_for_pid(pid)
        if command and is_dashboard_command(command):
            safe_print(f"Stopping existing Codex Usage Dashboard on port {port} (pid {pid}).")
            terminate_process(pid)
        else:
            blockers.append(f"{pid}: {command or 'unknown process'}")

    if blockers:
        joined = "; ".join(blockers)
        raise RuntimeError(f"Port {port} is already in use by a non-dashboard process: {joined}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Local read-only Codex usage dashboard.")
    parser.add_argument(
        "--codex-home",
        type=Path,
        action="append",
        default=None,
        help="Path to a Codex home directory. Can be provided multiple times.",
    )
    parser.add_argument(
        "--no-auto-windows",
        action="store_true",
        help="Do not auto-add the Windows ~/.codex directory when running under WSL.",
    )
    parser.add_argument("--host", default="127.0.0.1", help="Bind host. Defaults to 127.0.0.1.")
    parser.add_argument("--port", type=int, default=8765, help="Port to bind. Defaults to the fixed dashboard port 8765.")
    parser.add_argument("--open", action="store_true", help="Open the dashboard in the default browser.")
    parser.add_argument("--once", action="store_true", help="Scan once and print a short summary instead of serving the UI.")
    parser.add_argument("--json", action="store_true", help="With --once, print JSON.")
    parser.add_argument(
        "--export-snapshot",
        nargs="?",
        const="",
        default=None,
        help="Export this device's full Cousash JSON snapshot. Optionally pass an output path.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    sources = (
        codex_sources_from_homes(args.codex_home)
        if args.codex_home
        else default_codex_sources(include_windows=not args.no_auto_windows)
    )
    remote_store = RemoteSnapshotStore()
    analyzer = CodexUsageAnalyzer(sources, remote_store=remote_store)
    if args.export_snapshot is not None:
        body = analyzer.export_snapshot_json()
        output = Path(args.export_snapshot).expanduser() if args.export_snapshot else Path.cwd() / f"cousash-{remote_store.current_device_code}.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(body + "\n", encoding="utf-8")
        safe_print(str(output.resolve()))
        return 0

    if args.once:
        snapshot = analyzer.scan()
        if args.json:
            payload = {
                "generated_at": snapshot["generated_at"],
                "codex_home": snapshot["codex_home"],
                "codex_sources": snapshot.get("codex_sources", []),
                "summary": snapshot["summary"],
                "sessions": snapshot["sessions"],
            }
            safe_print(json.dumps(payload, ensure_ascii=False, indent=2))
        else:
            usage = snapshot["summary"]["usage"]
            safe_print(f"Codex homes: {snapshot['codex_home']}")
            safe_print(f"Sessions: {snapshot['summary']['session_count']}")
            safe_print(f"Total tokens: {usage['total_tokens']:,}")
            if snapshot["sessions"]:
                top = snapshot["sessions"][0]
                safe_print(f"Top session: {top.get('title', top.get('session_id'))} ({top['total_token_usage']['total_tokens']:,})")
        return 0

    port = args.port
    release_dashboard_port(port)
    try:
        server = FixedPortHTTPServer((args.host, port), make_handler(analyzer))
    except OSError as exc:
        raise RuntimeError(f"Port {port} is already in use and could not be released.") from exc
    url = f"http://{args.host}:{port}/"
    safe_print(f"Codex Usage Dashboard: {url}")
    safe_print(f"Codex homes: {analyzer.codex_home_display}")
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
