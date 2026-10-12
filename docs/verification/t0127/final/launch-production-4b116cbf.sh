#!/bin/sh
export VIAMIND_MCP_PORT=7964
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
exec /home/charlesmowbray/.cache/viamind/native-artifacts/t0127/viamind-4b116cbf --open /home/charlesmowbray/.cache/viamind/native-artifacts/t0127/production-fixture-4b116cbf.viamind.json > /home/charlesmowbray/.cache/viamind/native-artifacts/t0127/launch-production-4b116cbf.sh.log 2>&1
