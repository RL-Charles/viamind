# Bounded manufacturing export verification (T75)

[Master provenance](verification/t0075-master.json) records the full local Linux
gate: fmt, strict workspace all-target clippy,446 passing tests with one existing
ignored test, and workspace build. The final exact copied production binary was built
from bb6b61d on combined main c55dc7c, SHA256
`2b63011a0f05e628791bdac260c2b2005c102451c60a52ea40d9929304282cd1`.
Rust1.99 CI deprecated `fetch_update`; its behavior-preserving `try_update` rename
passes local Rust1.98.1 and [stable CI](https://github.com/RL-Charles/viamind/actions/runs/38126471644).
All four local gates and33 production checks were repeated after that fix.
The master retains historical evidence from the earlier binary. Binaries are excluded from git.

[Production report](verification/t0075-rust199-production-export.json) has33 passing CLI
and actual native HTTP checks. BOM and placement CSV and every Gerber/XNC/manifest
file are byte-identical to the prior production binary at e5c1144, SHA256
`17890de437ae1386ce2b05363f4a1aa912921ed0a3494c4c4a0202dae12659be`.
CSV is independently decoded with Python's CSV parser, preserving Unicode,
commas, quotes and embedded CRLF. Native HTTP files match the production CLI.

The same report verifies collisions, active-native protection, overbudget CLI
and native requests preserving existing outputs, no partial fabrication directory,
two-agent stale cursor failures, unchanged read-only session/status and exact undo
restoration after guard/budget edits. The native server is the actual production
application on7902, PID329724/window110379505456096; an environment-only launch
script selected its port. No inspection application was used.

[Fresh Cua metadata](verification/t0075-rust199-native-cua.json) and
[capture](verification/t0075-rust199-native-production.png) identify that process after
final undo. The compositor tiled this production window to45×59 and AT-SPI
exposes metadata only. This capture provides window identity, not form usability
or native input evidence. The actual existing desktop fabrication form regression
passes, including invalid input, shared output and unchanged design/status.

Additional Rust regressions exercise large invalid unrelated sheets, header-only
assembly exclusion of malformed large pad metadata, UTF-8/aggregate/count bounds,
shared busy admission/release, and cancellation while a blocking worker still owns
its snapshot. All supported byte formats and fabrication exclusions remain unchanged.

## Reproduction

Use a fresh directory that does not yet exist. Build the current binary and retain
an independent prior binary from e5c1144 for compatibility comparison:

```sh
cargo build --locked -p viamind
python3 docs/verification/t0075-export-smoke.py \
  --binary target/debug/viamind --baseline /absolute/path/to/prior-viamind \
  --project docs/verification/t0075-export.viamind.json \
  --directory /tmp/viamind-t75-fresh-check
```

For native checks, launch that current production binary with the same fixture and
a free coordinated port (`VIAMIND_MCP_PORT=7900`), then add
`--endpoint http://127.0.0.1:7900/mcp` to the command. Existing previews must be
preserved. The script creates only new evidence outputs and undoes its two guarded
agent edits. Its `--repo` option selects this checkout for the shared MCP client.

```sh
cargo test --locked -p viamind-fab --test limits
cargo test --locked -p viamind-mcp --test manufacturing_limits
cargo test --locked -p viamind-ui-egui --test fabrication
```
