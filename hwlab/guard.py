#!/usr/bin/env python3
"""PreToolUse hook for hwlab sessions: block eFuse burns, provisioning, prod and erase commands."""

import json
import re
import sys

BLOCKED = [
    (r"espefuse|burn[_-]|burn_efuse|efuse.*(write|burn)", "eFuse burns"),
    (r"\bprovision\b|full-provision|flash-bootloader|gen-keys|frostsnap_factory\s+--\s+(provision|gen-)", "provisioning"),
    (r"env=prod|\bprod\b.*secure-boot-key", "prod environment"),
    (r"erase-flash|erase-region|erase-parts|erase-device|\bjust\s+erase\b", "erase commands"),
]

command = json.load(sys.stdin).get("tool_input", {}).get("command", "")
for pattern, what in BLOCKED:
    if re.search(pattern, command, re.IGNORECASE):
        print(f"hwlab guard: blocked ({what}) in this worktree; ask Nick to run it.", file=sys.stderr)
        sys.exit(2)
