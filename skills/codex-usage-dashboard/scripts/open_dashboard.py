#!/usr/bin/env python3
"""Open the bundled Codex usage dashboard."""

from __future__ import annotations

import json
import platform
import selectors
import shutil
import subprocess
import sys
import time
import webbrowser
from pathlib import Path
from urllib.request import urlopen


HOST = "127.0.0.1"
PORT = 8765


def dashboard_url(port: int) -> str:
    return f"http://{HOST}:{port}/"


def dashboard_script() -> Path:
    return Path(__file__).resolve().with_name("codex_usage_dashboard.py")


def health_dashboard_url(port: int) -> str | None:
    url = dashboard_url(port)
    try:
        with urlopen(url + "api/health", timeout=0.4) as response:
            if response.status != 200:
                return None
            payload = json.loads(response.read(256).decode("utf-8"))
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


def parse_dashboard_url(line: str) -> str | None:
    prefixes = (
        "Codex Usage Dashboard: ",
        "Codex Usage Dashboard already running: ",
    )
    for prefix in prefixes:
        if line.startswith(prefix):
            return line.removeprefix(prefix).strip()
    return None


def read_dashboard_url(process: subprocess.Popen[str], timeout: float = 2.5) -> str | None:
    if process.stdout is None:
        return None

    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    deadline = time.monotonic() + timeout
    try:
        while time.monotonic() < deadline:
            remaining = max(0, deadline - time.monotonic())
            events = selector.select(remaining)
            if not events:
                break
            line = process.stdout.readline()
            if not line:
                if process.poll() is not None:
                    break
                continue
            url = parse_dashboard_url(line.strip())
            if url:
                return url
    finally:
        selector.close()
    return None


def start_dashboard() -> str | None:
    system = platform.system()
    script = str(dashboard_script())

    if system == "Windows":
        creationflags = 0
        for name in ("CREATE_NO_WINDOW", "CREATE_NEW_PROCESS_GROUP", "DETACHED_PROCESS"):
            creationflags |= getattr(subprocess, name, 0)
        subprocess.Popen(
            [windows_pythonw(), "-u", script, "--port", str(PORT), "--open"],
            cwd=str(Path(script).parent),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            close_fds=True,
            creationflags=creationflags,
        )
        return None

    python = sys.executable or shutil.which("python3") or shutil.which("python")
    if not python:
        raise RuntimeError("Python was not found.")

    process = subprocess.Popen(
        [python, "-u", script, "--port", str(PORT), "--open"],
        cwd=str(Path(script).parent),
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        bufsize=1,
        close_fds=True,
        start_new_session=True,
    )
    return read_dashboard_url(process)


def main() -> int:
    started_url = start_dashboard()
    if started_url:
        print(started_url)
        return 0

    for _ in range(16):
        time.sleep(0.15)
        running_url = dashboard_running_url()
        if running_url:
            print(running_url)
            return 0

    print(f"Dashboard starting. Try {dashboard_url(PORT)}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
