#!/usr/bin/env python3
"""Install a desktop launcher for the bundled Codex usage dashboard."""

from __future__ import annotations

import json
import platform
import stat
import subprocess
import sys
from pathlib import Path


APP_NAME = "Codex Usage Dashboard"


def scripts_dir() -> Path:
    return Path(__file__).resolve().parent


def skill_dir() -> Path:
    return scripts_dir().parent


def desktop_dir() -> Path:
    desktop = Path.home() / "Desktop"
    return desktop if desktop.exists() else Path.home()


def shell_quote(value: str) -> str:
    return "'" + value.replace("'", "'\\''") + "'"


def install_windows() -> Path:
    link = desktop_dir() / f"{APP_NAME}.lnk"
    opener = scripts_dir() / "open_dashboard.py"
    icon = skill_dir() / "assets" / "codex_usage_dashboard.ico"
    pythonw = Path(sys.executable).with_name("pythonw.exe")
    target = str(pythonw if pythonw.exists() else sys.executable)

    ps_script = f"""
$link = {json.dumps(str(link))}
$target = {json.dumps(target)}
$opener = {json.dumps(str(opener))}
$workdir = {json.dumps(str(scripts_dir()))}
$icon = {json.dumps(str(icon))}
$ws = New-Object -ComObject WScript.Shell
$shortcut = $ws.CreateShortcut($link)
$shortcut.TargetPath = $target
$shortcut.Arguments = '"' + $opener + '"'
$shortcut.WorkingDirectory = $workdir
$shortcut.Description = "Open the local Codex usage dashboard"
if (Test-Path $icon) {{
  $shortcut.IconLocation = $icon + ",0"
}}
$shortcut.Save()
"""
    subprocess.run(
        ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", ps_script],
        check=True,
    )
    return link


def install_macos() -> Path:
    launcher = desktop_dir() / f"{APP_NAME}.command"
    opener = scripts_dir() / "open_dashboard.py"
    content = f"""#!/bin/zsh
cd {shell_quote(str(scripts_dir()))}

if command -v python3 >/dev/null 2>&1; then
  exec python3 {shell_quote(str(opener))}
elif command -v python >/dev/null 2>&1; then
  exec python {shell_quote(str(opener))}
else
  echo "Python 3 was not found. Install Python 3 and run this shortcut again."
  read "?Press Return to close."
fi
"""
    launcher.write_text(content, encoding="utf-8")
    launcher.chmod(launcher.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return launcher


def main() -> int:
    system = platform.system()
    if system == "Windows":
        created = install_windows()
    elif system == "Darwin":
        created = install_macos()
    else:
        print(f"Unsupported OS for desktop shortcut: {system}", file=sys.stderr)
        print(f"Run instead: python3 {scripts_dir() / 'open_dashboard.py'}", file=sys.stderr)
        return 2

    print(f"Created: {created}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
