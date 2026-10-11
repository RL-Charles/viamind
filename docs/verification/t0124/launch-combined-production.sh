#!/bin/bash
set -euo pipefail
export VIAMIND_MCP_PORT=7960 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
exec /home/charlesmowbray/.cache/viamind/native-artifacts/t0124/viamind-33981e0532b7 --open /home/charlesmowbray/.cache/viamind/native-artifacts/t0124/owned-production.viamind.json > /home/charlesmowbray/.cache/viamind/native-artifacts/t0124/native-production-combined.log 2>&1
