"""Development-only differential oracle, read from a pinned historical commit.

No Python implementation is shipped or used by the application. Compatibility
checks may materialize this audited reference in a temporary directory. Git
history containing this revision is required only for these developer checks.
"""
from pathlib import Path
import hashlib
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
REVISION = 'd0d9497'
SHA256 = 'af792e659c6459aa2c3b805cdcfa3bd78a9679d1fec8610191d32d4394426641'
_temp = tempfile.TemporaryDirectory(prefix='dashboard-test-oracle-')
SOURCE = Path(_temp.name) / 'codex_usage_dashboard.py'
_data = subprocess.check_output(['git', 'show', f'{REVISION}:skills/codex-usage-dashboard/scripts/codex_usage_dashboard.py'], cwd=ROOT)
assert hashlib.sha256(_data).hexdigest() == SHA256, 'Historical test oracle changed'
SOURCE.write_bytes(_data)
