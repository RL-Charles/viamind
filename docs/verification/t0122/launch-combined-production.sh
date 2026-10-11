#!/bin/sh
set -eu
exec env VIAMIND_MCP_PORT=7953 /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/viamind-combined-6157a2b > /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/combined-production.log 2>&1
