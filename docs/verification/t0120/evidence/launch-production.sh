#!/bin/sh
export VIAMIND_MCP_PORT=7961
export TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
exec /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/viamind-2cbe6dd6e1bb-fresh --open /home/charlesmowbray/Development/viamind-t-0120/docs/verification/t0120/inspection-fixture.viamind.json > /home/charlesmowbray/.cache/viamind/native-artifacts/t0120/production-native.log 2>&1
