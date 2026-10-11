#!/bin/bash
set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0121
test "$(git rev-parse HEAD)" = "b9621579ef362de0a913bd5556ecee0d009a86bf"
test -z "$(git status --porcelain --untracked-files=all -- crates apps)"
export CARGO_TARGET_DIR=/home/charlesmowbray/.cache/viamind/build-target
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp PYTHONDONTWRITEBYTECODE=1
rg --files crates apps -g '*.rs' -0 | xargs -0 touch
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p viamind-doc --test footprint_holes --test pad_apertures --test save_load --test format_determinism --test footprint --test footprint_artwork --test schematic_links --test package_links --locked
cargo test -p viamind-engine --test footprint_holes_foundation --test pad_apertures --test footprint_artwork --locked
