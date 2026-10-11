# Native symbol publication under the current native lifecycle guard (T-0107)

## Public sources and scope

Reviewed 2026-10-11: the public [KiCad 9 symbol editor manual](https://docs.kicad.org/9.0/en/eeschema/eeschema.html#creating-and-editing-symbols) describes reusable symbol definitions and explicit library save. Viamind retains its independent rectangular single-unit native symbol format and existing explicit save/export workflows. No external symbols, code or assets are used.

The [Rust File documentation](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all) documents explicit file synchronization and warns that another process may modify files. The pinned MIT/Apache [tempfile 3.27.0 publication documentation](https://docs.rs/tempfile/3.27.0/tempfile/struct.NamedTempFile.html#method.persist_noclobber) specifies refusal when a destination exists, with platform-specific atomicity and cleanup limitations. Use the existing dependency and safe Rust APIs. Linux is the verified target. No universal atomicity, directory durability after power loss, protection against hostile parent-directory replacement, or control of external processes is claimed.

This follow-up changes the current native-path check at publication for both `save_symbol_template` and `export_symbol_template`. It leaves valid symbol bytes, file format/version, tool names/schemas, editor field identities, geometry, loader, separate placement and read-only admission unchanged. It adds no model/schema/dependency. The compact editor layout remains a separate concern.

## Bounded preparation and NEW publication

Retain existing borrowed writer admission and strict semantic validation: at most1024 pins,256KiB aggregate text,1MiB serialized file, and4096 encoded bytes per requested path. Resolve the destination's existing parent directory, require a filename, and reject an overlong resolved destination before creating a stage. Existing detailed symbol name/value/footprint/pin and exact integer Nm validation remains authoritative.

Provide a shared staged symbol writer used by detached save and source-symbol export. Validate and serialize the same native envelope/ordered pins/newline bytes, create a private temporary sibling, write all bytes and synchronize the file before publication. Preparation and file synchronization occur outside both the design mutex and native lifecycle mutex. The stage owns its destination and temporary file; no destination entry is created by staging. Ordinary error/drop paths clean up the stage using the existing temporary-file contract.

For engine publication, acquire the existing shared native-file lifecycle guard, which excludes native save/open/new/project replacement through the final path check and publication. Obtain the **current** active native path under that guard. Canonicalize it when possible and compare it with the resolved destination and any canonical destination. Refuse a match with an actionable active-native-project error before attempting destination publication. Do not use a path captured before staging as the publication guard.

Publish only a NEW destination with the existing no-clobber primitive. A file, symlink or other destination created after staging must be preserved; never truncate, replace or silently choose another name. Return the resolved published path only after success. Refusal changes no project/history/selection/status and does not write the destination. If a peer completed a legitimate native operation during staging, preserve that peer's resulting document/status/native bytes; compare against the state immediately before the guarded publication attempt. Failed preparation also leaves the destination absent or unchanged.

Hold the lifecycle mutex only for the latest-path check and final publication. The guard releases the design mutex before invoking publication, so ordinary design commands remain available. Publication must not reenter native lifecycle operations. This is an in-process coordination guarantee for operations using the shared handle; external filesystem writers remain subject to no-clobber refusal and the documented filesystem limitations.

## Save and export guard semantics

Detached save has no expected-project/cursor requirement and creates no design delta. A project change during staging does not by itself invalidate a detached definition; publication still protects the latest native path. Successful save returns the original definition name/pin count and resolved path.

Source-symbol export retains its existing exact project UUID/full source cursor/sheet and symbol identity checks when capturing one bounded definition under the design mutex. It does not clone a project/sheet. Subsequent ordinary design edits do not retroactively change the captured template. The result's `exported_cursor` identifies that capture, not a promise that the model remained unchanged until publication. Independently protect the latest native path at publication. Do not invent a second cursor check or mutate the model/history to export.

Existing desktop actions continue through the same engine methods/background worker and MCP tools. Writer tools remain blocked in agent read-only mode; trusted symbol load/snapshot observations remain available. No automatic apply or placed-instance update follows file completion.

## Required evidence

- Shared doc-stage tests: no destination before publish; exact successful bytes; existing and newly racing destination preserved; current native destination refusal; stage cleanup on refusal/drop; original bounds/strict format remain unchanged.
- Deterministic engine hooks between stage and publication for **both** save and export: a peer native save or guarded open makes the planned destination the current native path; the writer emits the active-native error and preserves the peer's exact bytes/full session/status. Include an externally created destination race and stage cleanup. Verify the lifecycle guard excludes concurrent native transitions through publication, without blocking ordinary commands on the design mutex.
- Export stale capture refuses before output; valid export returns the captured cursor and ordered exact template. Detached valid NEW save still succeeds from a dirty session without modifying it. Actual two-client HTTP and stdio writer/load tests, existing actual desktop save/export input tests and canonical tool catalog drift checks establish shared behavior and read-only admission.
- Linux fmt, strict workspace/all-target Clippy, workspace tests/build; retained unique production binary/report hashes and a self-closing readable native capture with explicit UI-pass/session provenance. Do not attribute deterministic test-hook races to production MCP scheduling. Fresh final combined CI and independent review precede merge.

Parent S034 multi-unit symbol acceptance remains open.
