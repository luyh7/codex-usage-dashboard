#!/usr/bin/env python3
"""Open the bundled Codex Usage Dashboard."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time
import webbrowser
from pathlib import Path
from urllib.request import urlopen

# importlib-based callers need the sibling module without modifying sys.path.
import importlib.util
_runtime_spec = importlib.util.spec_from_file_location("dashboard_runtime", Path(__file__).with_name("dashboard_runtime.py"))
runtime = importlib.util.module_from_spec(_runtime_spec)
_runtime_spec.loader.exec_module(runtime)


DEFAULT_HOST = "127.0.0.1"
PORT = 8765
LAUNCH_LOG_ENV = "COUSASH_DASHBOARD_LAUNCH_LOG"
STARTUP_TIMEOUT_SECONDS = 30.0
POLL_INTERVAL_SECONDS = 0.15


def dashboard_url(port: int, host: str = DEFAULT_HOST) -> str:
    return f"http://{host}:{port}/"


def dashboard_script() -> Path:
    return Path(__file__).resolve().with_name("codex_usage_dashboard.py")


def dashboard_log_path() -> Path:
    override = os.environ.get(LAUNCH_LOG_ENV)
    if override:
        return Path(override).expanduser()
    return Path(tempfile.gettempdir()) / "codex-usage-dashboard.log"


def health_dashboard_url(port: int, host: str = DEFAULT_HOST) -> str | None:
    url = dashboard_url(port, host)
    try:
        with urlopen(url + "api/health", timeout=0.4) as response:
            if response.status != 200:
                return None
            payload = json.loads(response.read().decode("utf-8"))
    except Exception:
        return None
    if (isinstance(payload, dict) and payload.get("app") == "codex-usage-dashboard"
        and payload.get("implementation") == "rust" and payload.get("protocol_version") == 1
        and payload.get("ui") in {"legacy", "svelte"}
        and {"paged-events-v1", "background-index-v1"}.issubset(payload.get("features", []))):
        return url
    features = payload.get("features") if isinstance(payload, dict) else None
    if (
        isinstance(payload, dict)
        and payload.get("app") == "codex-usage-dashboard"
        and isinstance(features, list)
        and "calendar-range-v2" in features
        and "calendar-month-year-picker-v1" in features
        and "calendar-month-year-token-totals-v1" in features
        and "calendar-incomplete-total-spinner-v1" in features
        and "calendar-incomplete-day-spinner-v1" in features
        and "calendar-per-day-load-state-v1" in features
        and "calendar-month-year-range-apply-v1" in features
        and "calendar-view-default-range-v1" in features
        and "multi-codex-home" in features
        and "windows-cwd-folder-name" in features
        and "project-grouped-default-view" in features
        and "project-compact-layout-v2" in features
        and "project-env-tag-in-conversation-column" in features
        and "git-worktree-project-grouping" in features
        and "remote-snapshot-import-v1" in features
        and "remote-snapshot-cache-v1" in features
        and "session-inventory-cache-v1" in features
        and "effective-dated-pricing-v1" in features
        and "gpt-6-astra-pricing-v1" in features
        and "gpt-6-family-pricing-v1" in features
        and "runtime-log-service-tier-v1" in features
        and "latest-service-tier-badge-v2" in features
        and "bounded-period-scan-v1" in features
        and "fork-aware-subagent-usage-v1" in features
        and "expandable-agent-task-rollups-v1" in features
        and "compact-agent-task-tree-v1" in features
        and "aligned-agent-task-tree-v1" in features
        and "main-agent-child-usage-row-v1" in features
        and "aligned-task-title-gutter-v1" in features
        and "project-session-indent-v1" in features
        and "clickable-task-rows-and-gutter-v1" in features
        and "main-agent-title-alignment-v1" in features
        and "compact-subagent-indent-v1" in features
        and "project-folder-aggregate-columns-v1" in features
        and "filtered-summary-metrics-v1" in features
        and "fast-rollout-projection-v1" in features
        and "snapshot-detail-token-v1" in features
        and "persistent-parse-cache-v1" in features
        and "append-resume-v1" in features
        and "parallel-cold-parse-v1" in features
        and "conditional-session-refresh-v1" in features
        and "client-detail-cache-v1" in features
        and "component-snapshot-cache-v1" in features
        and "fast-mode-pricing-v1" in features
        and "ultrafast-mode-pricing-v1" in features
        and "subagent-fast-mode-inheritance-v1" in features
    ):
        return url
    return None


def dashboard_running_url(host: str = DEFAULT_HOST) -> str | None:
    return health_dashboard_url(PORT, host)


def windows_pythonw() -> str:
    exe = Path(sys.executable)
    sibling = exe.with_name("pythonw.exe")
    if sibling.exists():
        return str(sibling)
    found = shutil.which("pythonw.exe")
    return found or str(exe)


def open_browser(url: str) -> None:
    try:
        webbrowser.open(url, new=2)
    except Exception:
        pass


def wait_for_dashboard_url(
    process: subprocess.Popen[bytes] | None,
    host: str = DEFAULT_HOST,
    timeout: float = STARTUP_TIMEOUT_SECONDS,
    interval: float = POLL_INTERVAL_SECONDS,
) -> str | None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        running_url = dashboard_running_url(host)
        if running_url:
            return running_url
        if process is not None and process.poll() is not None:
            return None
        time.sleep(interval)
    return dashboard_running_url(host)


def start_dashboard(host: str, native=None, extra_args=()) -> subprocess.Popen[bytes] | None:
    system = platform.system()
    script = str(dashboard_script())
    env = runtime.launch_environment() if native else os.environ.copy()
    command = [str(native[0]), "--host", host, "--port", str(PORT), *extra_args] if native else None
    log_path = dashboard_log_path()
    log_path.parent.mkdir(parents=True, exist_ok=True)

    if system == "Windows":
        creationflags = 0
        for name in ("CREATE_NO_WINDOW", "CREATE_NEW_PROCESS_GROUP", "DETACHED_PROCESS"):
            creationflags |= getattr(subprocess, name, 0)
        with log_path.open("ab") as log_file:
            process = subprocess.Popen(
                command or [windows_pythonw(), "-u", script, "--host", host, "--port", str(PORT), *extra_args],
                cwd=str(Path(script).parent),
                stdin=subprocess.DEVNULL,
                stdout=log_file,
                stderr=subprocess.STDOUT,
                close_fds=True,
                creationflags=creationflags,
                env=env,
            )
        if native: runtime.write_state({"pid":process.pid,"build_id":native[1]["build_id"],"instance_id":env["COUSASH_INSTANCE_ID"],"shutdown_token":env["COUSASH_SHUTDOWN_TOKEN"]})
        return process

    python = sys.executable or shutil.which("python3") or shutil.which("python")
    if not python:
        raise RuntimeError("Python was not found.")

    with log_path.open("ab") as log_file:
        process = subprocess.Popen(
            command or [python, "-u", script, "--host", host, "--port", str(PORT), *extra_args],
            cwd=str(Path(script).parent),
            stdin=subprocess.DEVNULL,
            stdout=log_file,
            stderr=subprocess.STDOUT,
            close_fds=True,
            start_new_session=True,
            env=env,
        )
    if native: runtime.write_state({"pid":process.pid,"build_id":native[1]["build_id"],"instance_id":env["COUSASH_INSTANCE_ID"],"shutdown_token":env["COUSASH_SHUTDOWN_TOKEN"]})
    return process


def main() -> int:
    parser = argparse.ArgumentParser(description="Open the bundled Codex Usage Dashboard.")
    parser.add_argument("--host", default=DEFAULT_HOST, help="Bind host. Defaults to 127.0.0.1.")
    parser.add_argument("--backend", choices=["auto","rust","python"], default=os.environ.get("COUSASH_BACKEND","auto"))
    parser.add_argument("--no-browser", action="store_true")
    parser.add_argument("--codex-home", action="append", default=[])
    parser.add_argument("--no-auto-windows", action="store_true")
    args = parser.parse_args()
    try:
        native = runtime.native_binary() if args.backend != "python" else None
        if args.backend == "rust" and native is None:
            raise RuntimeError("No verified native artifact for this platform. Run install_runtime.py with a release manifest, or use --backend python. Launch never downloads files.")
        existing = runtime.health(args.host)
        desired = native[1]["build_id"] if native else hashlib.sha256(dashboard_script().read_bytes()).hexdigest()
        mismatch = existing.get("build_id") != desired
        if args.codex_home:
            mismatch = mismatch or sorted(existing.get("codex_homes", [])) != sorted(str(Path(p).expanduser().resolve()) for p in args.codex_home)
        if existing and mismatch: runtime.release_owned(args.host, dashboard_script())
        elif not existing or not health_dashboard_url(PORT,args.host): runtime.release_owned(args.host, dashboard_script())
    except (OSError,ValueError,RuntimeError,subprocess.SubprocessError) as error:
        print(str(error),file=sys.stderr)
        return 1

    running_url = dashboard_running_url(args.host)
    if running_url:
        if not args.no_browser: open_browser(running_url)
        print(running_url)
        return 0

    extra = [item for home in args.codex_home for item in ("--codex-home", home)]
    if args.no_auto_windows: extra.append("--no-auto-windows")
    process = start_dashboard(args.host, native, extra)
    started_url = wait_for_dashboard_url(process, args.host)
    if started_url:
        if not args.no_browser: open_browser(started_url)
        print(started_url)
        return 0

    if process is not None and process.poll() is not None:
        print(
            f"Dashboard failed to start (exit code {process.returncode}). See {dashboard_log_path()}.",
            file=sys.stderr,
        )
        return 1

    print(f"Dashboard is still starting. Try {dashboard_url(PORT, args.host)}. Logs: {dashboard_log_path()}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
