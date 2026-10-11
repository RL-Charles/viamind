# Saved output-job verification

T-0084 source `026518e3d358034d44d94c987f150e4daa2a720d`, based on main `83c5dd6`,
passed Linux fmt, strict workspace/all-target clippy, **523 workspace tests**
(with one existing ignored test), and workspace build. The subsequent clean
static-routing integration onto `bf8d455` preserved all additive actions/hooks;
its only conflict was the README append, resolved by preserving both links.
Final combined CI is recorded on the PR before merge.

[Master provenance](verification/t0084-master.json) distinguishes the actual
production CLI binary from the fixed1600×1000 native inspection wrapper. The
production artifact was deliberately stripped of debug symbols after its
successful build, copied to a unique home-cache file and verified by hash. The
wrapper uses production `ViamindApp`, `EngineHandle` and MCP; its fixed viewport
makes native evidence readable without changing production window behavior.

## Evidence

[Native report](verification/t0084-native-output-job.json) records **45 passing
checks**: display-free production CLI runs, canonical descriptors, all ten
format/configuration bytes equal between CLI and native HTTP, complete release
manifest/cursor parity, standalone generic CSV and all seven fabrication bytes,
prior verified production fabrication parity, NEW/native/file/symlink collision
preservation, malformed/duplicate/deep/oversize/FIFO jobs, missing parents, two
agent sessions, stale guard, undo, full shared-state preservation and cleanup.
CLI starts a distinct stream, so release manifests compare after substituting the
actual native source cursor; their other metadata agrees exactly. The desktop
and live MCP integration test uses one engine and compares all eleven bytes.

The fresh [native capture](verification/t0084-native-output-job.png) shows readable
job controls and actual board geometry after native undo. The accompanying
[Cua report](verification/t0084-native-cua.json) reports unavailable AT-SPI/X11
metadata fallback; it does not prove input or checked state. Actual pointer/text
save/load/run, button/result scrolling within640×400, errors, disabled controls,
retained hidden completion, duplicate suppression and fallible spawn recovery are
covered by egui_kittest. Native raw input is not claimed.

Core tests additionally prove one permit for the combined job, unrelated invalid
schematic exclusion, coherent capture despite later edits,100001-component
rejection before copy, bounded JSON/CSV appends, post-stage failure cleanup and a
raced competing root preserved by no-replace rename. The canceled-awaiter test
keeps the actual job snapshot inside a running blocking worker until publication.

## Reproduce

Build the production application at the recorded source, run the four Linux
gates, and preserve a unique binary before another worktree changes the cache:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
strip --strip-debug -o /path/to/production-verified target/debug/viamind
```

Use a writable `TMPDIR` and home-cache artifacts on hosts with `/tmp` quotas.
Create an external Cargo package using the tracked
[manifest template](verification/t0084-native-Cargo.toml.in) with `@REPO@` replaced
by this checkout's absolute path, and
[inspection source](verification/t0084-native-inspection.rs) as `src/main.rs`.
Build with `cargo rustc --manifest-path /path/to/inspection/Cargo.toml --offline --
-C link-arg=-Wl,--strip-debug`, then copy its binary to a unique path. Launch it
with a **copy** of `fixtures/fabrication/asymmetric.viamind.json` and an unused
local port. Existing previews are preserved; the recorded wrapper uses7910.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 docs/verification/t0084-output-job-smoke.py \
  --binary /path/to/production-verified \
  --project fixtures/fabrication/asymmetric.viamind.json \
  --directory /path/to/NEW-evidence-directory \
  --endpoint http://127.0.0.1:7910/mcp --repo "$PWD"
```

Optional `--previous-binary` adds prior verified fabrication-byte comparisons;
without it the script still verifies current standalone/CLI/native parity and
failure behavior. Capture fresh window state after the script finishes. Binaries
and temporary release directories are not committed.

This fixed profile does not add manufacturer centroids, sourcing/population
metadata, vendor certification, inner copper/pours, archives, views or validation
gates. Full manufacturing acceptance remains open beyond supported output scope.
