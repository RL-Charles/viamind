# Transient PCB command branches and visual review (T113)

## Sources and scope

Written before implementation from public behavioral references consulted 2026-10-11:

- [Git branch](https://git-scm.com/docs/git-branch) and [Git diff](https://git-scm.com/docs/git-diff): prepare changes against a captured base, inspect added/modified/removed content, and explicitly merge or discard. This feature uses an independent in-memory design sandbox; it does not invoke Git or implement Git's merge algorithm.
- [KiCad 9 PCB editor](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html): visible PCB objects, explicit editing and undo provide the public interaction vocabulary.
- [MCP tools, 2025-03-26](https://modelcontextprotocol.io/specification/2025-03-26/server/tools): advertised strict tool schemas, explicit results/errors and application-side authorization. A caller-supplied label is not connection identity.
- Existing independent Viamind [session events](session-events.md), [interactive placement](interactive-placement.md), [copper editing](copper-tracks-vias.md) and [read-only admission](agent-read-only-mode.md) contracts remain authoritative for those features.

Provide a bounded agent sandbox for PCB edits and a human visual review workflow. Ordinary live apply remains the default. This is voluntary branch preparation, not S027's enforced approval mode: an admitted live agent can explicitly merge its own branch or continue using existing live tools. Startup read-only admission cannot create, modify, merge or discard branches. No scripts, network calls, filesystem writes, persistent branches, automatic rebase or general conflict resolution are introduced.

## Captured base and bounded storage

At most four branches exist per shared application process. A branch has a fresh UUID, exact original project UUID/full design-selection cursor, an independent revision starting at zero, a bounded display label, and a transport-owned creator identity. Its candidate is an isolated Project copy; its accumulated changes are coalesced original engine deltas. Project/history/selection/dirty state/native bytes and design/DRC journals remain unchanged by branch creation, application, inspection or discard.

Before any project copy, count borrowed structural rows across the entire project: board components, pads, tracks, vias, named policy rows, schematic sheets, symbols, pins, wires, junctions and labels. Refuse over 5,000 rows. Serialize the borrowed project to a bounded counting writer, stopping above 2 MiB without building an unbounded buffer; only a project that fits may be cloned. The same limits apply to every tentative result before storing it. This is an intentionally small-design sandbox, not a 100k-board performance claim. Whole-project capture preserves every unrelated field, including assembly and schematic data, without changing its schema.

At most 128 coalesced changed objects and 2 MiB of serialized coalesced deltas may be retained per branch. Count before copying/storing results. An individual branch request permits at most 64 commands and 64 KiB of serialized input, checked against the borrowed JSON before a second decode/copy. Command names fit 64 ASCII snake-case bytes; optional labels fit 128 UTF-8 bytes, are nonblank and control-free. No arbitrary caller-supplied Project, delta, owner identity, base replacement or merge result is accepted.

One nonwaiting process-wide branch-work permit bounds creation/application. Acquire before capture/copy and retain inside the actual worker, including canceled MCP awaiters. The branch manager marks one branch busy while its private candidate is being prepared. Release its short state mutex before running commands; hold no live document/native/DRC mutex while doing sandbox command work. Failed spawn, validation, command execution or final capacity validation clears busy and preserves the previous branch revision/candidate/deltas exactly. Discard and merge of a busy branch refuse.

Branches are process-local and have a five-minute idle lifetime, renewed only after successful creator application. Creation starts the deadline. Reads, errors and other clients do not renew it. Expiration is checked lazily on each branch operation and never evicts an active worker's slot. An expired/unknown UUID is an actionable error. New/Open changes the design stream; branches from older streams are stale and cannot be applied or merged. A stale branch may be inspected/discarded until expiration, but its ghost is hidden on the unrelated document. No old worker can recreate a discarded/expired branch or publish into a replacement branch.

## Original command execution and stable identities

Expose strict create, apply, inspect, list, diff, merge and discard operations. Create requires expected project and exact full live cursor; it returns branch identity/revision/source. Apply requires branch identity and expected branch revision plus commands. All errors are all-or-nothing. Each successful nonempty apply increments the branch revision once. A net no-op is refused without revision or deadline changes.

Only these original built-in PCB commands are accepted initially:

`add_part`, `move_part`, `delete_part`, `update_part`, `transform_parts`, `set_part_pads`, `add_track`, `delete_track`, `add_via`, `delete_via`.

Use the original registry dispatch/validation implementations, with every command's optimistic expected cursor compared to the branch's original captured live cursor. Earlier deliberate steps in the same branch apply are visible to subsequent steps. Object guards compare against the current private candidate. Command-generated IDs are created once in the sandbox, appear in returned changes, and are retained exactly at merge/undo/redo. Never rerun creation commands at merge. A failure in any step rolls back that entire branch application; it must not affect earlier successful branch applications or the live engine.

After each individual private command, validate candidate row/byte limits and incrementally coalesce/check delta row/byte limits before executing the next command. Do not retain every intermediate full-object delta across a 64-command batch. One command may temporarily produce before/after snapshots of the bounded candidate; no later command runs if its result exceeds retained limits. Reuse the original bounded command input and candidate budgets rather than growing an unbounded private history.

These commands retain their existing validation and scoped guarantees. Sandbox creation/application does not imply complete DRC, continuity or manufacturing approval. Routing, named-policy editing, outlines, footprint replacement, schematic edits, native lifecycle, export, selection and history commands are not accepted in this initial branch command list. They remain independently available through their existing live APIs.

Use creator transport identity from the trusted MCP call context; same-name clients remain distinct. A live agent may apply, inspect candidate state, merge and discard only its own branch. A human may inspect, merge or discard any branch. The bounded list exposes branch UUID, creator display label, source, revision, changed-object counts and stale/busy/remaining-lifetime facts, not candidate contents. A read-only agent may list and inspect bounded diffs, but receives no mutation handlers. Direct API test actors are explicit types, not deserializable tool arguments. This is process-local ownership, not authentication of arbitrary remote clients or approval security.

Inspection returns the bounded candidate Board and branch/source/revision metadata to its creator or the human, preserving integer Nm. It does not clone live history or saved baselines. Diff uses deterministic object-kind/UUID order, strict offset/limit with default 32 and maximum 128, and returns coalesced before/after object values plus total/next-page facts. Limit the serialized diff result to the same 2 MiB delta cap. List/diff are read-only observations; reading never consumes another observer's state.

## Guarded merge and undo

Merge requires branch identity, expected branch revision and the exact original project/full cursor. Under the live engine lock, verify current project/full cursor and every coalesced delta's original before value. Any peer edit, selection change, undo/redo or same-UUID reopen rejects the entire merge with a rebuild/discard instruction. Saving unchanged geometry alone does not advance the cursor and does not make a branch stale.

Only the internal prepared branch object can supply merge deltas. Atomically apply the approved coalesced deltas as one original-engine transaction with original creator attribution and a bounded branch label. Record one undo entry and one design event, preserve stable IDs and all unrelated fields, update ordinary dirty/selection-pruning behavior, and invoke existing post-change automatic DRC outside its usual locks. Remove the branch only after successful commit. Undo restores the exact pre-merge Project; redo restores the exact candidate edits and IDs. Repeated merge refuses. A failed stale merge retains its branch for inspection/discard and changes no live field or journal.

Use one lock order whenever both live engine and branch state are needed: engine then branch state. Never acquire the live engine while holding branch state. Worker preparation has neither lock; final publication revalidates identity/revision/busy/source stream and capacity before replacing branch state. Normal human editing remains available during worker preparation. A canceled transport awaiter does not detach admission or create a second worker.

## Desktop review

Add a discoverable Agent branches review entry. A scrollable compact form lists bounded branches, creator/source/revision/counts and stale/busy notices. Selecting a branch displays a local read-only visual diff on the PCB canvas: green added/new geometry and red removed/old geometry, with an explicit legend and an unchecked-DRC notice. Modified objects show both old and new geometry. Reuse production component/pad/track/via geometry and layer transforms; ghost geometry does not participate in hit-testing or shared selection and does not hide ordinary live geometry. Cache the derived ghost scene by branch revision, and refresh only after an explicit branch-state observation or lifecycle change.

The human can review added/removed/modified object summaries, show/hide the ghost, Merge or Discard. These actions use shared typed lifecycle operations and matching MCP tools. A stale source disables Merge; explicit Refresh updates the notice without rebasing or altering candidate content. Preserve stale/error text and the selected branch until explicit user recovery. New/Open, switching away from PCB, opening an incompatible active editor or losing the branch clears the local ghost. Reviewing or canceling the dialog changes no shared design field. Existing live edits remain usable and simply stale the branch's merge guard.

At 640×400 logical pixels, creator/source/legend, branch selection, Merge/Discard and actionable error/completion text remain reachable through ordinary scrolling. Native verification must show readable changed copper/component ghosts and separately readable final merge/completion controls; accessibility metadata alone is not proof of painted visibility. No physical display cadence or OS-input claim follows from local egui inspection events.

## Required verification

- Independent pre-code review of this public contract and the live engine transaction/registry/transport identity integration.
- Original fixed-ID fixtures prove whole-project borrowed budgets before copying, command/input/delta/branch/lease caps, no-op and atomic failed steps, exact base-cursor comparison within successive/batched branch steps, generated-ID retention and distinct same-name connection ownership.
- Deterministic blocked private worker proves live edit liveness, one nonwaiting permit retained after canceled awaits, failure recovery, source replacement refusal and no recreation after invalidation. Lease tests use injected monotonic time; no long sleeps.
- One merge event/undo step, byte-exact undo/redo Project, stable IDs, dirty/selection/history/native invariance before merge, stale peer selection/edit/reopen refusal, repeated merge refusal and full original transport status/refusal invariants.
- Initialized two-client production MCP lifecycle plus read-only admission, complete candidate/diff/undo checks and bounded observations; actual compact desktop review/ghost/merge/discard/error inputs with exact executable/native/framebuffer provenance.
- Four Linux workspace gates, strict all-target Clippy, actual canonical MCP catalog discovery/drift, isolated agent workflow, fresh final integrated-head CI and independent final review before merge. Final integration follows T112 and preserves all predecessor changes and historical proof identities.
