#!/bin/bash
# Post-cargo steps of final-gates.sh for 4b116cbf. final2 stopped at the clean-tree assertion after
# GATE_BUILD_PASS because an untracked docs/verification/t0127/verify-artifacts.py had been written
# during the run; no Rust input changed. Rust sources are asserted equal to HEAD here.
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0127
export PYTHONDONTWRITEBYTECODE=1 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
E=/home/charlesmowbray/.cache/viamind/native-artifacts/t0127; TAG=4b116cbf
test "$(git rev-parse HEAD)" = 4b116cbf693886fa5ffacd456d6fb20c366a1cbd
test -z "$(git status --porcelain=v1 --untracked-files=no)"
git status --porcelain=v1
sha256sum -c $E/binaries-$TAG.sha256
python3 -m unittest discover -s scripts/tests -p 'test_mcp_reference.py'
python3 scripts/generate-mcp-reference.py --binary $E/viamind-$TAG --check docs/generated --report $E/catalog-$TAG-report
python3 scripts/mcp-agent-workflow.py --binary $E/viamind-$TAG --output $E/workflow-$TAG
python3 scripts/verify-schematic-buses-native.py --repo /home/charlesmowbray/Development/viamind-t-0127 --binary $E/viamind-$TAG --directory $E/stdio-$TAG
date -u +POST_END_%FT%TZ
echo POST_GATES_COMPLETE
