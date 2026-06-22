#!/usr/bin/env python3
"""Export the bundled Cousash snapshot from the project root."""

from __future__ import annotations

import runpy
import sys
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "skills" / "codex-usage-dashboard" / "scripts" / "export_snapshot.py"


if __name__ == "__main__":
    original_argv = sys.argv[:]
    try:
        sys.argv = [str(SCRIPT), *sys.argv[1:]]
        runpy.run_path(str(SCRIPT), run_name="__main__")
    finally:
        sys.argv = original_argv
