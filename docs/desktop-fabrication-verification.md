# Fabrication controls: Linux and native verification

T-0082 hardens the existing retained asynchronous fabrication export. It does not
change Gerber/XNC geometry, native schema or manufacturer acceptance. The
[control specification](specs/desktop-fabrication-controls.md) was committed before
implementation and cites public Rust/egui documentation.

## Exact artifacts

The [master report](verification/t0082-master.json) identifies the artifact source,
production binary and fixed-size inspection binary hashes, full Linux gates,
native checks and capture. The inspected binaries were copied while the shared
build lock was held. Later documentation or unrelated integration commits do not
change those artifact identities.

All four required Linux gates pass: fmt, strict workspace all-target clippy,
500 tests plus one existing ignored test, and workspace build. The optional
post-build copy then exceeded the temporary-filesystem quota. Only its incomplete
copy was removed; `strip --strip-debug` produced the unique production artifact
in the home cache. The inspection wrapper links with `-Wl,--strip-debug` and uses
a home-cache `TMPDIR`. Both exact hashes are recorded and both artifacts passed
the smoke checks. No gate success is inferred from that failed copy.

The production application binary is tested through its actual no-display CLI.
Native HTTP runs a fixed 1600×1000 inspection wrapper around the production
`ViamindApp`, `EngineHandle` and MCP server on 127.0.0.1:7908. Its only differences
are fixed viewport sizing and opening the existing fabrication form at startup.
Production window resizing is unchanged. The wrapper is not mislabeled as the
production CLI binary.

The source44e29fa [Linux CI](https://github.com/RL-Charles/viamind/actions/runs/38129796451)
is green. Subsequent integration onto pin/pad navigation main3c55c9f preserves all
UI hooks/actions and both README links; fresh combined CI is required before merge.

The [native report](verification/t0082-native-fabrication.json) compares all seven
files for default 0 and 0.050001 mm mask expansion between production CLI and actual
native HTTP exports, including the manifest and full exported cursor. It also
compares bytes with the prior verified production artifact. Independent Gerber
and XNC geometry validation remains in the original fabrication increment;
this control increment preserves those accepted bytes.

Tests cover actual 640×400 pointer/text entry, vertical scrolling to Export and
result/error, library/live-MCP file equality, full UUID/sequence cursor, newer-edit
notice, collision/native protection, invalid input, stale-before-validation and
busy admission without shared state changes. Controlled unit tests prove disabled
pending controls, retained hidden/disconnected workers and recoverable named-thread
startup failure. The native smoke also exercises two distinct clients, stale
requests, undo and complete project/status preservation.

## Native capture limits

The [fresh screenshot](verification/t0082-native-fabrication.png) and
[Cua metadata](verification/t0082-native-cua.json) verify the actual readable form
in that inspection wrapper. Cua reported unavailable AT-SPI and
recovered X11 window metadata only. Actual pointer/text, disabled controls and
compact-scroll claims come from `egui_kittest`; this capture proves visible
content and is not claimed as native click input.
Existing previews and desktop configuration are preserved.

## Reproduce

Run the workspace Linux fmt, strict all-target clippy, tests and build gates.
Use a fresh temporary directory for the wrapper source and replace `@REPO@` in the
[manifest template](verification/t0082-native-Cargo.toml.in) with the checkout path;
copy [inspection source](verification/t0082-native-inspection.rs) as `src/main.rs`.
Build with `cargo build --manifest-path /path/to/temporary/Cargo.toml`. Launch that
binary with the explicit fixture path and an unused loopback port, keeping other
previews running:

```sh
/path/to/viamind-fabrication-export-inspection \
  "$PWD/fixtures/fabrication/asymmetric.viamind.json" 7908
python3 docs/verification/t0082-fabrication-smoke.py \
  --binary /path/to/verified/viamind \
  --project fixtures/fabrication/asymmetric.viamind.json \
  --directory /existing/parent/new-evidence \
  --endpoint http://127.0.0.1:7908/mcp --repo "$PWD"
```

The evidence directory must not exist. Optional `--previous-binary` enables the
prior-version byte comparison recorded in our report; its SHA is recorded
separately. Without it, the same script still verifies production CLI/native HTTP
parity and all failures. Binary artifacts are excluded from git.

The package remains scoped to two outer copper/mask layers, explicit SMD pads,
straight tracks, through vias and polygon profile/cutouts. Legacy previews,
inner copper/pours, paste/silkscreen/component holes, sourcing metadata and actual
fab upload acceptance remain open. Application exit can interrupt a worker.
