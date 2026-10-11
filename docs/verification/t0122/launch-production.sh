#!/bin/sh
set -eu
exec env VIAMIND_MCP_PORT=7952 /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/viamind-2c8417bb3fde > /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/production.log 2>&1
