---
name: codex-usage-dashboard
description: Open the local Codex/Cousash usage dashboard or export this device's Cousash snapshot for manual remote import.
---

For `/cousash-open`, opening the Codex usage dashboard, or managing imported remote device data:

```bash
python3 scripts/open_dashboard.py
```

For `/cousash-export` or exporting the current device for another dashboard:

```bash
python3 scripts/export_snapshot.py
```

The export command prints the generated `cousash-<device-short-code>.json` path. The dashboard can import that JSON file from its remote data button.
