#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0127
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 PYTHONDONTWRITEBYTECODE=1 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
E=/home/charlesmowbray/.cache/viamind/native-artifacts/t0127
H=$(git rev-parse --short=8 HEAD); git rev-parse HEAD; test -z "$(git status --porcelain=v1)"
exec 9>/tmp/viamind-workspace-gates.lock
flock 9
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo build -p viamind --locked
cp "$CARGO_TARGET_DIR/debug/viamind" $E/viamind-catalog-$H
flock -u 9
sha256sum $E/viamind-catalog-$H
python3 scripts/generate-mcp-reference.py --binary $E/viamind-catalog-$H --output $E/catalog-$H --report $E/catalog-$H-report
python3 scripts/generate-mcp-reference.py --binary $E/viamind-catalog-$H --check docs/generated || echo CHECK_DIFFERS
