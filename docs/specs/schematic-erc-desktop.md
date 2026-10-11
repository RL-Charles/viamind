# Desktop native pin ERC workflow

## Public behavior sources

Written before implementation, from public documentation only:

- [Schematic ERC](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#electrical-rules-checking): explicitly run checks, inspect error/warning results, navigate to the related schematic item, and configure compatibility and diagnostic severity. The manual also explains why correctly declared pin types matter and why ERC cannot detect every error.
- [ERC severity and pin conflicts](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#erc-violation-severity-and-pin-conflicts-map): independently configurable diagnostic severities and pin compatibility.
- Viamind's independent [native ERC policy](schematic-erc.md), exact graph and session cursor APIs define the supported subset. No implementation code from another product is used.

## Supported interaction

A desktop Electrical rule check window is available for an open project, with a schematic-side entry. Run ERC calls the same read-only engine API as the `run_erc` MCP tool. The form edits the 66 unordered native pin pairs and independent unconnected-input, undriven-input, missing-power and no-connect-attachment severities. Each severity is allow, warning or error. Reset restores the documented native defaults. Maximum returned issues is bounded 1..10000. Form settings remain local to the window/session and are not saved into the project.

Run uses one background worker and a single bounded completion channel. Only one job can be outstanding; repeated clicks cannot create a queue or threads. Engine graph capture and processing retain the engine's explicit limits. The worker does not own the application and cannot mutate design, history, dirty state or selection. Closing/replacing a document cannot apply an old completion to its replacement. Dropping the window/application leaves at most the already bounded engine query to finish; it never blocks the UI waiting for a join.

Show errors and warnings, stable net names/IDs and stable pin/symbol/sheet IDs. Show report coverage, unsupported checks and work/result stop reasons. Incomplete zero-issue results must say incomplete, never all clear. Complete zero-issue results say no issues in supported checks, never full ERC certification. Display the effective policy used by the result separately from the current form if changed.

## Freshness and navigation

Each result is tied to its project ID and source event cursor (including session stream). A current cursor mismatch visibly marks it stale. A changed form marks the result policy stale. Both stale cases disable navigation until a new run; a report may remain visible for comparison within the same document. Document replacement, including reopening the same persisted project ID in a new stream, clears prior results and rejects pending completions. Switching PCB/schematic view does not alter the document or its history.

A fresh result can navigate to an identified pin's sheet and symbol, switch to schematic mode and center the exact pin endpoint. Navigation revalidates project/stream/cursor and the symbol/pin identity against current immutable model state before changing local view state. It cancels a pending board gesture through the normal mode switch. It does not change shared engine selection or model state. MCP users have the same diagnostic identities through `run_erc` and schematic query tools; camera/window choices are local view state.

## Verification

Use deterministic controller tests for one outstanding job, policy changes, edits during runs, document replacement and same-ID reopen. Actual egui input tests must run the form and navigate valid issues, reject stale navigation and verify unchanged session/history. Production Linux inspection must show readable native results and clear coverage/freshness. Run Linux fmt, strict all-target clippy, workspace tests and build. No automatic ERC, hierarchy, hidden power, absent marker/flag editing, report-file export or full-ERC claim is included.
