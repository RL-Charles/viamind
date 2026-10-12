"""Verify every hash indexed by the T-0126 combined-gate master."""

import hashlib
import json
import sys
from pathlib import Path

root = Path(__file__).resolve().parent
master = json.loads((root / "master.json").read_text())
failures = 0
for name, expected in master["files"].items():
    actual = hashlib.sha256((root / name).read_bytes()).hexdigest()
    if actual != expected:
        failures += 1
        print(f"MISMATCH {name}")
binary = Path(master["binary"]["path"])
if "--retained" in sys.argv[1:]:
    if hashlib.sha256(binary.read_bytes()).hexdigest() != master["binary"]["sha256"]:
        failures += 1
        print(f"MISMATCH {binary}")
print(f"verified {len(master['files'])} files" if not failures else f"{failures} mismatches")
sys.exit(1 if failures else 0)
