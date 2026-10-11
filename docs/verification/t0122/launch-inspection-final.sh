#!/bin/sh
set -eu
exec env -u WAYLAND_DISPLAY /home/charlesmowbray/.cache/viamind/native-artifacts/t0122/inspection-fa9793b "$1" "$2" "$3" > "$4" 2>&1
