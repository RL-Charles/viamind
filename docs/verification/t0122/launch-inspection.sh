#!/bin/sh
set -eu
exec env -u WAYLAND_DISPLAY /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/inspection-2c8417bb3fde "$1" "$2" "$3" > "$4" 2>&1
