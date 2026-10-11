# Agent instructions

## Work tracking (Sherpa)

Work in this repo is tracked with Sherpa (`shp`, tickets in `.sherpa/`). Hooks enforce it:
edits are blocked until a ticket is claimed, and stopping with unlogged changes asks for a log.

1. `shp context` to see what to work on; `shp show <T-id>` for the spec and history.
2. `shp start <T-id>` before changing code. If nothing fits: `shp new ticket "…" -p <S-id>`, then start it.
3. Log as you go: `shp log "headline" -f <file>`, with `-k decision|verify|blocker` and `-d` for details.
4. Keep the ticket readable: `shp now "one-line state"`, tick acceptance checks with evidence
   (`shp check <n> -e <sha|path|PR>`), record measurables (`shp measure "<name>" <value> --target <goal>`).
   Stories and epics need a Visual: `shp visual <S-id> --stdin` (a generated one shows until then).
5. Open the PR yourself (`gh pr create`); Sherpa links it to the ticket. `shp sync` links PRs made elsewhere.
6. Finish: `shp submit -m "what changed" --verified "how it was checked"`. It warns about anything missing and lands in agent review.
   Reviewing agent work when asked: `shp review <T-id> --pass -m "what you checked"` (on to human review) or `--reject -m "what to fix"`.
   Only a human marks work done.
7. Never edit `.sherpa/` by hand. Commits get a `Ticket: T-xxxx` trailer automatically.

## Parallel work

Several agents work at once, one lane each. Read [docs/PARALLEL.md](docs/PARALLEL.md) before claiming a ticket.

## Project overview

Viamind is an open-source, agent-driven PCB design suite in Rust (MIT, clean-room).
A human in the desktop UI and an agent over MCP drive the same command engine, so
everything one can do the other can do too, and every change is undoable.

## Crate map

| Crate | Purpose |
| --- | --- |
| `crates/geom` (`viamind_geom`) | Geometry primitives, integer nanometre units (`Nm`), spatial index |
| `crates/doc` | Design model: projects, schematic sheets, boards, rules |
| `crates/engine` | Typed commands, transactions, undo/redo, event log |
| `crates/mcp` | Agent harness: MCP server exposing every engine command as a tool |
| `crates/ui-egui` | Desktop UI on egui/eframe |
| `crates/render` | Canvas rendering of schematics and boards |
| `crates/router` | Interactive and guided routing |
| `crates/drc` | Design rule checking |
| `crates/fab` | Manufacturing outputs (Gerber, drill, IPC-2581, ODB++, STEP) |
| `apps/viamind` | The application binary |

Dependencies point one way (`a ← b` means b depends on a); never add an edge back up:

```
geom ← doc ← engine ← mcp ──────┐
                    ← ui-egui ──┴─ apps/viamind
geom, doc ← render ← ui-egui
geom, doc ← router, drc, fab
```

`geom` depends on no other Viamind crate. `doc` knows nothing about commands, UI or MCP.

## Build, test, lint

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all                                         # CI runs: cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p viamind                                    # launch the app
```

All four checks (build, test, fmt, clippy) must pass before `shp submit`; CI
(`.github/workflows/ci.yml`) runs them on Linux, macOS and Windows. Lints are set
in the root `Cargo.toml` (`[workspace.lints]`), `clippy.toml` and `rustfmt.toml`;
`unsafe_code` is forbidden. On Linux, eframe needs the X11/Wayland/xkbcommon/GL
development packages (see the CI workflow for the list).

## Conventions

- **Units:** all model coordinates and lengths are integer nanometres (`viamind_geom::Nm`).
  Convert to `f32`/`f64` only at the render or I/O boundary, never store floats in the model.
- **Commands:** every change to the design model goes through an engine command
  (`crates/engine`), so it is undoable, logged and reachable by agents. UI and MCP code
  never mutate `doc` types directly.
- **Agent parity:** every UI action must have an MCP tool. Adding a UI action without the
  matching command and tool is incomplete work.
- **Lanes:** stay inside your lane's crates (see [docs/PARALLEL.md](docs/PARALLEL.md));
  shared files (`Cargo.toml` workspace deps, `Cargo.lock`) change in small commits.
- Crates are named `viamind-<name>`; new dependencies go in `[workspace.dependencies]`
  and are referenced with `.workspace = true`.

## Clean room

Viamind is an independent implementation; read [CLEAN_ROOM.md](CLEAN_ROOM.md) before
working on any feature that mirrors existing EDA tools.

- Allowed: observed behaviour of software you are licensed to use, public docs and videos,
  open standards (IPC-2581, ODB++, Gerber X3, IPC-7351, STEP), compatibly licensed open source.
- Forbidden: decompiling or debugging proprietary binaries, copying proprietary code,
  libraries, icons or UI assets, and proprietary trademarks in product, crate or feature names.
- Process: write a behavioural spec in `docs/specs/` citing public sources only, then
  implement from the spec alone.

## Git rules

- Work on a branch per ticket (`t-xxxx`) in your own worktree; open PRs against `main`.
- Stage paths explicitly (`git add <path> …`). Never `git add -A` or `git add .`.
- Never commit `.sherpa/` from a feature branch: Sherpa state is shared between worktrees.
- Commits get a `Ticket: T-xxxx` trailer from the hook; do not add one by hand.
- Do not merge your own PR; a human merges and marks the ticket done.
