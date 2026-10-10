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
