#!/usr/bin/env python3
"""Export this device's Cousash JSON snapshot."""

from __future__ import annotations

import runpy
import sys
from pathlib import Path


def main() -> int:
    script = Path(__file__).resolve().with_name("codex_usage_dashboard.py")
    original_argv = sys.argv[:]
    try:
        sys.argv = [str(script), "--export-snapshot", *sys.argv[1:]]
        runpy.run_path(str(script), run_name="__main__")
    finally:
        sys.argv = original_argv
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
