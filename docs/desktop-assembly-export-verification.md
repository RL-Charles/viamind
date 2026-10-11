# Desktop assembly CSV verification (T78)

[Master provenance](verification/t0078-master.json) records exact source
7f6672a on merged main 5a91aa0. Linux fmt, strict workspace all-target clippy,
461 passing tests with one existing ignored test, and workspace build pass.
[Source CI](https://github.com/RL-Charles/viamind/actions/runs/38128022227) is green.
Production CLI binary SHA256:
`cc5fb8f44540234c6110ebf490f8f016073006d97cc846e7b5d98607169ece35`.

[Native report](verification/t0078-native-export.json) records 23 passing production
CLI/native HTTP checks: both kinds preserve byte parity and full source cursors,
independent Python CSV parsing preserves Unicode/quotes/CRLF, BOM grouping and
bottom placement conventions match the fixture, explicit overwrite works, NEW
collisions and active-native overwrite preserve bytes, and exports leave shared
session/status unchanged. Two transport sessions verify stale cursor rejection
without output and exact undo restoration.

The native server on 7905 is a fixed-size inspection wrapper using production
`ViamindApp`, `EngineHandle` and production MCP registration, PID 448706/window
110379508130208. Its separate binary SHA256 is
`6cc4f13fba150262e22861aca6546da04f870d964ee5465564a63a991523edba`.
The [wrapper source](verification/t0078-native-inspection.rs) changes only the
viewport and opens the real CSV form through its public entry point. Production
application resize behavior is unchanged; the production CLI binary is tested
separately. Binaries stay out of git.

[Fresh Cua metadata](verification/t0078-native-cua.json) and the
[readable native capture](verification/t0078-native-export.png) verify the real
CSV form after the final undo. AT-SPI is metadata-only on this desktop; native
raw clicks are unsupported. Actual pointer/text input is covered by three egui
input tests, including File-menu access, both kinds, live MCP/library byte parity,
explicit overwrite, preserved collisions/native/budget/stale failures, and visible
Export/result/error controls after scrolling at 640×400.

Controlled production-path regressions pause after CSV staging while a second
editor opens the destination as a native project; desktop and MCP publication
then reject without changing that file or the newly opened session. A separate
native lifecycle regression holds a save-as after writing and before marking its
active path: publication waits for completion and sees the new native path.
Ordinary engine commands remain available during that guard. Rendering, temporary
writing and fsync happen before it; only path checks and atomic publication happen
inside. Hidden workers remain retained, duplicate starts are suppressed, and a
disconnected worker produces a recoverable error.

After those exact artifact checks, the branch was rebased onto interactive-wire
main391fd00. Both additive wire and CSV action helpers were preserved; the CSV
implementation and captured artifact evidence remain unchanged. Fresh combined
CI and focused desktop/parity checks verify that integration.

## Reproduction

Use a new directory for each script run. The wrapper source takes a project path
and optional port. Build it as a temporary binary with the tracked manifest:

```sh
python3 - <<'PY'
from pathlib import Path
repo = Path.cwd().resolve()
preview = Path('/tmp/viamind-t78-preview-reproduce')
(preview / 'src').mkdir(parents=True, exist_ok=False)
source = repo / 'docs/verification'
(preview / 'Cargo.toml').write_text((source / 't0078-native-Cargo.toml.in').read_text().replace('@REPO@', str(repo)))
(preview / 'src/main.rs').write_text((source / 't0078-native-inspection.rs').read_text())
PY
cargo build --locked -p viamind
cargo run --manifest-path /tmp/viamind-t78-preview-reproduce/Cargo.toml -- \
  "$PWD/docs/verification/t0075-export.viamind.json" 7905
```

Choose an unused coordinated port and preserve existing previews. In another
terminal, run the portable check against that endpoint:

```sh
python3 docs/verification/t0078-export-smoke.py \
  --binary target/debug/viamind \
  --project docs/verification/t0075-export.viamind.json \
  --directory /tmp/viamind-t78-fresh-evidence \
  --endpoint http://127.0.0.1:7905/mcp
cargo test --locked -p viamind-ui-egui --test assembly_export --test command_parity
cargo test --locked -p viamind-ui-egui --lib assembly_export::tests
cargo test --locked -p viamind-mcp --lib assembly::tests
cargo test --locked -p viamind-engine --lib native_publication_waits
```

The fixture is synthetic. Generic interchange limits and excluded sourcing,
centroid and manufacturer acceptance remain described in the
[desktop guide](desktop-assembly-export.md).
