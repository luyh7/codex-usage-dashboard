#!/usr/bin/env python3
"""Open the bundled Codex usage dashboard from the project root."""

from __future__ import annotations

import runpy
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "skills" / "codex-usage-dashboard" / "scripts" / "open_dashboard.py"


if __name__ == "__main__":
    runpy.run_path(str(SCRIPT), run_name="__main__")
