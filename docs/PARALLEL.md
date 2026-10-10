# Parallel work plan (P0–P1)

Agents work in separate git worktrees, one ticket each, and merge through PRs.
Tickets are tracked in Sherpa (`shp context`); dependencies are recorded with `shp depends`.

## Lanes

Each lane owns a set of crates, so agents in different lanes rarely touch the same files.

| Lane | Focus | Owns | Tickets |
| --- | --- | --- | --- |
| A | Model and engine | `crates/doc`, `crates/engine` | T-0016 → T-0008 → T-0010 → T-0011, T-0012 → T-0013, T-0015; T-0014 → T-0009 |
| B | Geometry and rendering | `crates/geom`, `crates/render`, `crates/ui-egui` | T-0017 → T-0018, T-0019; T-0020 → T-0021; T-0022 |
| C | Agent harness | `crates/mcp` | T-0023 → T-0024 → T-0025; T-0026 |
| D | Repo and tooling | root files, `.github/`, `docs/` | T-0002, T-0003, T-0004, T-0005, T-0006, T-0007 |

`Nm` lives in `crates/geom` but is built by lane A (T-0016) because the model needs it first.

## Waves

A ticket moves from `backlog` to `ready` when everything it depends on is merged.

```
Wave 1  T-0001 crate layout ─┬─ T-0004 NOTICE    T-0005 PR template    T-0007 corpus   (no deps)
                             │
Wave 2  (after T-0001)       ├─ A: T-0016 Nm
                             ├─ B: T-0020 app shell + canvas
                             ├─ C: T-0023 MCP skeleton
                             └─ D: T-0002 lints, T-0003 CI, T-0006 AGENTS.md
Wave 3  A: T-0008 model types        B: T-0017 geometry, T-0021 layers
Wave 4  A: T-0010 commands, T-0014   B: T-0018, T-0019
Wave 5  A: T-0011, T-0012, T-0009    B: T-0022 selection    C: T-0024 MCP tools from schemas
Wave 6  A: T-0013, T-0015            C: T-0025 parity test
Wave 7  C: T-0026 live session (P1 gate: agent places a resistor, user undoes it)
```

At most 4 agents run at once (one per lane), which matches the widest wave.

## Rules for each agent

1. `shp start <T-id>` in your own worktree (`git worktree add ../viamind-<T-id> -b <T-id>`).
2. Stay inside your lane's crates; a change elsewhere goes in a separate ticket or a note to that lane.
3. Shared files (`Cargo.toml` workspace deps, `Cargo.lock`) change in small commits; rebase before the PR.
4. `cargo fmt`, `cargo clippy --workspace -- -D warnings` and `cargo test --workspace` pass before `shp submit`.
5. Open the PR with `gh pr create`; a human merges and marks the ticket done.
