# Checked output-job verification

T-0095 adds a saved `checked_outer_2layer` profile. It runs production scoped DRC
against the exact immutable board used for manufacturing output and publishes a
NEW release only when coverage is complete and no scoped violations remain.
This increment proves explicit saved request baselines. Persisted document
named-net rules are a separately tracked T-0097 integration; they are not claimed
by this evidence. Passing scoped checks is not full DFM, electrical continuity or
manufacturer acceptance.

## Production application and CLI

The retained production executable was built from
`3deab921233c1c648001ffcbb80dc324009a6d1b`, based on main
`0d958743c4f85c0a7165de869a43b9ffa21e4b36`.
The four local Linux gates passed: 654 tests passed, none failed and one ignored,
format checking, strict workspace/all-target Clippy and workspace build.
The [exact artifact-source CI](https://github.com/RL-Charles/viamind/actions/runs/38145197262)
also passed. Binary hashes, source identities and build-log hashes are in the
[master record](verification/t0095-master.json).

The unchanged production application opened a scratch copy of the tracked
[asymmetric board](../fixtures/fabrication/asymmetric.viamind.json), with actual
HTTP MCP on port 7923. The production no-display CLI and two initialized agent
clients passed [28 checks](verification/t0095-production-report.json). They
verified twelve fixed checked-release files, report and release cursor coherence,
exact report equality with standalone production DRC, and output byte equality
between clients. CLI comparison normalizes only the independent source cursor
in the validation and release manifests; ten format/config files match exactly.

Actual edge-clearance violations and an actually truncated DRC result both
refused publication. NEW collisions preserved existing bytes, external pass/report
fields were rejected, and ordinary failures left no staging directory. Exports
and refusals preserved the complete shared session. A separate peer edit and undo
advanced sequence 0 to 2 while restoring the exact project and clean native
status; that final cursor is not described as unchanged. Ten existing generic
format/config files matched the retained T-0092 production executable exactly.
Independent session cursors exclude release-manifest bytes from that historical
comparison.

## Current desktop and compact input

After the full artifact build, the branch incorporated unrelated main changes and
changed a desktop sentence to describe current explicit request baselines without
implying persisted document rules. A separate single-job private-cache refresh
forced all own Rust inputs fresh, repeated the four output-job UI tests, strict
workspace/all-target Clippy, format checking and workspace build, then built a
separate inspection executable. It did not alter the shared target or the retained
production process. The master distinguishes this current desktop build from the
earlier full 654-test production artifact.

The [inspection harness](verification/t0095-native-inspection.rs) embeds production
`ViamindApp` in a fixed 1600×1000 viewport, opens Saved output jobs and uses
paint-derived label bounds for two local egui pointer events selecting the checked
profile, then three local input frames drag the window corner to reveal all
controls. It starts no MCP listener and verifies complete session equality after
each production UI update. The [fresh native image](verification/t0095-native-final.png)
is an actual eframe WGPU Screenshot event, with the checked profile and explicit
baseline controls visible. Its [framebuffer report](verification/t0095-native-framebuffer-report.json)
records two profile button events, three resize input frames, all controls fully
visible and unchanged complete session state. Opaque P6 RGB
pixels were converted losslessly to PNG and independently decoded back to the same
RGB bytes. The inspection host closes itself after syncing the NEW capture and
flushing its report. No Cua screenshot, global input, performance or production
viewport-size claim is made.

The separate actual 640×400 `egui_kittest` regression selects the checked profile,
edits all rule fields, exercises strict save failure and recovery, saves/loads,
scrolls to Run and the result, compares twelve outputs with live MCP, and verifies
collision recovery. Pending-input tests verify that both profile controls and all
seven rule controls are disabled during retained work and re-enabled afterward.

## Final combined reference integration

After merging the MCP-reference tooling, a clean rebase onto main `f7d73811` and
fresh compilation of every own Rust crate produced a unique combined executable
at source `a93d9ff671a86ad73fdbb0254a2c55514539310f`. Its binary hash equals the
current-label production build. The merged changes contain no production Rust
changes. Formatting and workspace build passed; [canonical discovery](verification/t0095-reference-report.json)
regenerated both reference files for 81 actual application tools. Thirteen tooling
tests and [18 isolated agent workflow checks](verification/t0095-workflow-report.json)
passed. A subsequent read-only drift check matched the committed references.
Final exact-head combined CI is recorded on PR94 before merge; these records keep
the original production, current desktop and combined reference proofs separate.

## Reproduce

Build using the four commands in [AGENTS.md](../AGENTS.md). Copy the asymmetric
fixture to a scratch native path and start the production app with an unused MCP
port. Run the portable probe against it and a NEW evidence directory:

```sh
python3 docs/verification/t0095-checked-job-smoke.py \
  --binary /absolute/path/to/viamind \
  --previous-binary /absolute/path/to/retained-t0092-viamind \
  --project /scratch/board.viamind.json \
  --directory /scratch/new-evidence \
  --repo "$PWD" \
  --endpoint http://127.0.0.1:7923/mcp
```

The previous executable is required for historical generic-byte comparisons.
Its exact identity is in the master record. For native inspection, expand `@REPO@`
in the [manifest template](verification/t0095-native-Cargo.toml.in), copy the
harness to `src/main.rs`, build and run with a scratch native path and NEW `.ppm`
path. Capture the stdout JSON report; a successful Screenshot event writes and
syncs the bounded PPM before closing. Convert to PNG and verify exact decoded RGB
equality. Binaries and scratch releases are retained locally, not committed.

The [public-source specification](specs/checked-output-jobs.md) and
[usage guide](checked-output-jobs.md) state strict settings, bounds and exclusions.
