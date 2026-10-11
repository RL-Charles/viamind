#!/bin/sh
set -eu
exec env -u WAYLAND_DISPLAY /home/charlesmowbray/.cache/viamind/native-artifacts/t0114/inspection-fb9be124977b "$1" "$2" "$3" > "$4" 2>&1
