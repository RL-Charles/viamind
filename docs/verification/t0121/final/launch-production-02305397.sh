#!/bin/sh
export VIAMIND_MCP_PORT=7963
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
exec /home/charlesmowbray/.cache/viamind/native-artifacts/t0121/viamind-02305397 --open /home/charlesmowbray/.cache/viamind/native-artifacts/t0121/inspection-fixture.viamind.json > /home/charlesmowbray/.cache/viamind/native-artifacts/t0121/launch-production-02305397.sh.log 2>&1
