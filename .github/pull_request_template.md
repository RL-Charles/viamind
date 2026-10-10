## Summary

<!-- What changed and why, in a few lines. -->

## Sherpa ticket

Ticket: T-xxxx

## How it was verified

<!-- Commands run, tests added, screenshots, or manual steps. -->

## Checks

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo test --workspace` passes

## Clean-room checklist

See [CLEAN_ROOM.md](../CLEAN_ROOM.md). Tick every box, or mark it N/A with a reason.

- [ ] Behaviour specs in `docs/specs/` cite **public sources only**, with links (docs, tutorials, videos, open standards)
- [ ] No code derived from decompiling, disassembling, or debugging proprietary binaries
- [ ] No proprietary code, libraries, icons, UI assets, or data copied
- [ ] No proprietary trademarks in product, crate, module, or feature names
- [ ] No GPL code copied (Viamind is MIT; KiCad is GPL, so its code must not be copied)
- [ ] Third-party code, data, or assets use compatible licences and are recorded in `NOTICE` / `ATTRIBUTION.md`
