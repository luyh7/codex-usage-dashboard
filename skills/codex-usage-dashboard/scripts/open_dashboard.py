#!/usr/bin/env python3
"""Open the bundled Codex usage dashboard.

This script is intentionally standard-library only. It starts the dashboard as
a detached local process and returns quickly so Codex can continue responding.
"""

from __future__ import annotations

import json
import os
import platform
import shutil
import subprocess
import sys
import time
import webbrowser
from pathlib import Path
from urllib.request import urlopen


HOST = "127.0.0.1"
PORT = 8765
URL = f"http://{HOST}:{PORT}/"


def dashboard_script() -> Path:
    return Path(__file__).resolve().with_name("codex_usage_dashboard.py")


def dashboard_running() -> bool:
    try:
        with urlopen(URL + "api/sessions", timeout=0.7) as response:
            if response.status != 200:
                return False
            payload = json.loads(response.read().decode("utf-8"))
    except Exception:
        return False
    return isinstance(payload, dict) and isinstance(payload.get("summary"), dict)


def windows_pythonw() -> str:
    exe = Path(sys.executable)
    sibling = exe.with_name("pythonw.exe")
    if sibling.exists():
        return str(sibling)
    found = shutil.which("pythonw.exe")
    return found or str(exe)


def start_dashboard() -> None:
    system = platform.system()
    script = str(dashboard_script())

    if system == "Windows":
        creationflags = 0
        for name in ("CREATE_NO_WINDOW", "CREATE_NEW_PROCESS_GROUP", "DETACHED_PROCESS"):
            creationflags |= getattr(subprocess, name, 0)
        subprocess.Popen(
            [windows_pythonw(), script, "--port", str(PORT), "--open"],
            cwd=str(Path(script).parent),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            close_fds=True,
            creationflags=creationflags,
        )
        return

    python = sys.executable or shutil.which("python3") or shutil.which("python")
    if not python:
        raise RuntimeError("Python was not found.")

    subprocess.Popen(
        [python, script, "--port", str(PORT), "--open"],
        cwd=str(Path(script).parent),
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        close_fds=True,
        start_new_session=True,
    )


def main() -> int:
    if dashboard_running():
        webbrowser.open(URL, new=2)
        print(f"Dashboard already running: {URL}")
        return 0

    start_dashboard()
    for _ in range(20):
        time.sleep(0.25)
        if dashboard_running():
            print(f"Dashboard opened: {URL}")
            return 0

    print(f"Dashboard starting. If the browser did not open, visit {URL}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
