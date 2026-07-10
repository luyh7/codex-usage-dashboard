#!/usr/bin/env python3
"""Open the bundled Codex usage dashboard."""

from __future__ import annotations

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


HOST = "127.0.0.1"
PORT = 8765
LAUNCH_LOG_ENV = "COUSASH_DASHBOARD_LAUNCH_LOG"
STARTUP_TIMEOUT_SECONDS = 30.0
POLL_INTERVAL_SECONDS = 0.15


def dashboard_url(port: int) -> str:
    return f"http://{HOST}:{port}/"


def dashboard_script() -> Path:
    return Path(__file__).resolve().with_name("codex_usage_dashboard.py")


def dashboard_log_path() -> Path:
    override = os.environ.get(LAUNCH_LOG_ENV)
    if override:
        return Path(override).expanduser()
    return Path(tempfile.gettempdir()) / "codex-usage-dashboard.log"


def health_dashboard_url(port: int) -> str | None:
    url = dashboard_url(port)
    try:
        with urlopen(url + "api/health", timeout=0.4) as response:
            if response.status != 200:
                return None
            payload = json.loads(response.read().decode("utf-8"))
    except Exception:
        return None
    features = payload.get("features") if isinstance(payload, dict) else None
    if (
        isinstance(payload, dict)
        and payload.get("app") == "codex-usage-dashboard"
        and isinstance(features, list)
        and "calendar-range-v2" in features
        and "multi-codex-home" in features
        and "windows-cwd-folder-name" in features
        and "project-grouped-default-view" in features
        and "project-compact-layout-v2" in features
        and "project-env-tag-in-conversation-column" in features
        and "git-worktree-project-grouping" in features
        and "remote-snapshot-import-v1" in features
        and "effective-dated-pricing-v1" in features
    ):
        return url
    return None


def dashboard_running_url() -> str | None:
    return health_dashboard_url(PORT)


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
    timeout: float = STARTUP_TIMEOUT_SECONDS,
    interval: float = POLL_INTERVAL_SECONDS,
) -> str | None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        running_url = dashboard_running_url()
        if running_url:
            return running_url
        if process is not None and process.poll() is not None:
            return None
        time.sleep(interval)
    return dashboard_running_url()


def start_dashboard() -> subprocess.Popen[bytes] | None:
    system = platform.system()
    script = str(dashboard_script())
    log_path = dashboard_log_path()
    log_path.parent.mkdir(parents=True, exist_ok=True)

    if system == "Windows":
        creationflags = 0
        for name in ("CREATE_NO_WINDOW", "CREATE_NEW_PROCESS_GROUP", "DETACHED_PROCESS"):
            creationflags |= getattr(subprocess, name, 0)
        with log_path.open("ab") as log_file:
            return subprocess.Popen(
                [windows_pythonw(), "-u", script, "--port", str(PORT)],
                cwd=str(Path(script).parent),
                stdin=subprocess.DEVNULL,
                stdout=log_file,
                stderr=subprocess.STDOUT,
                close_fds=True,
                creationflags=creationflags,
            )

    python = sys.executable or shutil.which("python3") or shutil.which("python")
    if not python:
        raise RuntimeError("Python was not found.")

    with log_path.open("ab") as log_file:
        return subprocess.Popen(
            [python, "-u", script, "--port", str(PORT)],
            cwd=str(Path(script).parent),
            stdin=subprocess.DEVNULL,
            stdout=log_file,
            stderr=subprocess.STDOUT,
            close_fds=True,
            start_new_session=True,
        )


def main() -> int:
    running_url = dashboard_running_url()
    if running_url:
        open_browser(running_url)
        print(running_url)
        return 0

    process = start_dashboard()
    started_url = wait_for_dashboard_url(process)
    if started_url:
        open_browser(started_url)
        print(started_url)
        return 0

    if process is not None and process.poll() is not None:
        print(
            f"Dashboard failed to start (exit code {process.returncode}). See {dashboard_log_path()}.",
            file=sys.stderr,
        )
        return 1

    print(f"Dashboard is still starting. Try {dashboard_url(PORT)}. Logs: {dashboard_log_path()}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
