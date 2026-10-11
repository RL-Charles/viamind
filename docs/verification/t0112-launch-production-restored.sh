#!/usr/bin/env bash
set -eu
export VIAMIND_MCP_PORT=7942 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
exec /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/production-46ba551-verified --agent-read-only --open /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/production-project.json > /home/charlesmowbray/.cache/viamind/native-artifacts/t0112/production-native-restored.log 2>&1
