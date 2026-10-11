# Bounded saved-release consistency inspection (T112)

## Public sources and independent contract

Written before implementation, using public references consulted 2026-10-11:

- [RFC 8259, sections 4 and 9](https://www.rfc-editor.org/rfc/rfc8259): JSON member uniqueness/interoperability and explicit parser limits. This reader deliberately rejects duplicate and unknown structural members.
- [RFC 4180, section 2](https://www.rfc-editor.org/rfc/rfc4180): comma-separated fields, quoted multiline fields and doubled quote escaping. The supported original Viamind CSV dialect is UTF-8 with CRLF record delimiters and embedded quoted CR/LF preserved.
- [Rustix openat](https://docs.rs/rustix/latest/rustix/fs/fn.openat.html), [open flags](https://docs.rs/rustix/latest/rustix/fs/struct.OFlags.html), and [Rust descriptor metadata](https://doc.rust-lang.org/std/fs/struct.Metadata.html): safe descriptor-relative read-only opening and observed file type/length/timestamp checks. No unsafe code or new dependency is required.
- [KiCad 9 CLI jobsets](https://docs.kicad.org/9.0/en/cli/cli.html): saved jobs coordinate outputs and make failures visible. This independent increment inspects only Viamind's existing release format; it does not implement another application's format or certify manufacturing outputs.
- Existing independent [saved-job contract](saved-output-jobs.md), [calibrated companion](calibrated-output-job.md) and [matched populated companion](matched-populated-output-job.md) define the exact supported bytes, filenames and identity digest.

This is a detached, read-only consistency check. Existing settings, release versions, manifest serialization and output files remain byte-identical. No design/model/native/event schema expansion is introduced.

## Existing repository assessment

`crates/fab/src/output_job.rs` publishes a complete staged directory with fixed filenames using Linux `renameat_with(NOREPLACE)` after file/subdirectory fsync. `ReleaseManifest.files` contains relative paths and byte lengths excluding the release manifest itself. It contains no authenticated file-content digests. The optional populated summary carries only a component-set digest. Existing public `ReleaseManifest`/summary deserialization is permissive and its `BTreeMap` cannot detect repeated JSON keys; it is not an adequate untrusted reader contract.

`load_output_job` already opens a detached regular file with RDONLY/NONBLOCK/NOFOLLOW/CLOEXEC and a 64 KiB cap. It cannot anchor nested release entries to a retained root directory descriptor. Existing fabrication overlay reads explicit individual paths, allows final symlink resolution and has independent 4/16 MiB limits; reusing it for a complete release would change that contract. This increment adds its own purpose-specific bounded reader and strict structural types; it does not alter those readers or publication.

## Fixed supported release layouts

Accept release-manifest format `viamind-output-release`, versions 1, 2 or 3, and strict job.json format `viamind-output-job` with exactly the existing generic/checked profile/version/flag matrix. Unknown or duplicate keys, null optional summaries, invalid UUID/cursor, contradictory version/summary/profile, malformed bounds or unsupported layout reject the entire inspection.

The exact manifest file set is:

- `job.json`, `assembly/bom.csv`, `assembly/placement.csv`;
- `fabrication/top-copper.gbr`, `fabrication/bottom-copper.gbr`, `fabrication/top-mask.gbr`, `fabrication/bottom-mask.gbr`, `fabrication/profile.gbr`, `fabrication/plated-through.xnc`, `fabrication/manifest.json`;
- version 2/3 add `assembly/calibrated-placement.csv`; version 3 additionally adds `assembly/populated-bom.csv`;
- checked profile adds `validation/drc.json`.

The release manifest itself must not appear in `files`. No manifest-supplied path is used for opening: derive only these constant known paths after strict layout validation. Reject absolute/traversal/separator aliases, unknown entries, duplicate entries and absent required files. Do not recursively enumerate or inspect unrelated/unlisted filesystem entries; the successful result explicitly says these were not checked.

Release and job versions must match. Calibrated summary path/count/convention and populated summary paths/count/lowercase 64-hex digest must match their existing version contracts. Release project/cursor are historical provenance, not a comparison with a current document. Parse fabrication/manifest.json with strict bounded fields; its project UUID, fixed seven-file list, nonnegative bounded counts and mask-expansion integer must agree with release/job facts. Preserve bounded exclusion text for readable disclosure, without asserting those claims were recomputed.

Checked `validation/drc.json`, Gerber/XNC and older generic assembly files receive byte-count/file-type checks only. Their geometry, DRC authorization, syntax and sourcing semantics are explicitly not independently validated in this increment. Presence of a checked artifact is never treated as a new DRC pass.

## Safe bounded filesystem observation

Input is one explicit root path, 1–4096 encoded bytes. Linux only. Open the chosen final root as RDONLY/DIRECTORY/NOFOLLOW/CLOEXEC; already traversed user path parents may resolve normally. Retain this descriptor. Open known immediate subdirectories relative to it with DIRECTORY/NOFOLLOW, then each constant filename relative to the retained corresponding descriptor with RDONLY/NONBLOCK/NOFOLLOW/CLOEXEC. Verify every opened file is regular before reading; FIFO/device/socket/symlink substitution must refuse without blocking. No writes, creates, renames, deletions, executable hooks or source side effects occur.

This inspector accepts a deliberately bounded subset of otherwise exportable releases. Its per-file caps are inspection policy: generic job tables can use the writer's remaining 192 MiB release budget, so a valid larger export may require a different reader and is explicitly refused here. No claim is made that every existing export fits this inspector.

Limits before allocation: at most 13 manifest entries (14 release files including the manifest); release/job/fabrication manifest at most 64 KiB each; each assembly CSV at most 32 MiB; validation at most 16 MiB; fabrication files combined at most 128 MiB; complete observed release at most 192 MiB. Metadata count limits mirror current exports: 100000 components, 200000 fabrication primitives, at most 16 exclusion strings of 4096 bytes each. Declared sizes and checked sums must fit before per-file reads. Stream nonparsed files with a fixed buffer; do not retain their payloads.

One nonwaiting process-wide inspection permit is acquired before filesystem reads and retained inside the actual blocking/retained worker, including canceled MCP awaiters. It is distinct from manufacturing admission; inspection does not capture a Project or hold document/native locks. Read parsed bytes with limit+1 guards. Retain opened descriptors and check type/device/inode/length/modified/changed metadata before and after reading; re-open each constant entry through its original directory descriptor at final validation and verify identity/metadata. At completion, explicitly reopen the chosen root path with DIRECTORY/NOFOLLOW and compare its device/inode/metadata with the retained root. Reopen each known subdirectory through the original root descriptor and compare its device/inode/metadata with the retained subdirectory. Comparing only old descriptor metadata is insufficient to detect swapped path entries. Recheck retained descriptors and manifest bytes as well. An observed replacement, resize, truncation or metadata change rejects the whole result.

These checks detect ordinary observed races, not an atomic cross-file filesystem snapshot or hostile mutation that restores metadata. Files can change after return. Same-length edits to unchecked content can pass by design. No authenticated content-hash equality, original-export authenticity, current-board equivalence or filesystem immutability is claimed. Access-time changes caused by read-only filesystem access are outside document/file-payload invariance.

## Bounded calibrated/populated CSV checks

Version 1 does not parse assembly tables. Version 2 parses the existing calibrated-placement header and rows: exact columns, valid unique UUIDs, nonempty case-insensitive-unique references under the existing calibrated rules, explicit mounting (`surface_mount`, `through_hole`, `hybrid`), top/bottom side, signed fixed decimal coordinates with at most six fractional places, parsed using checked scaled integer arithmetic into Nm within ±99999.999999 mm. Reject exponent notation, nonfinite spellings and integer overflow. Parse rotation using checked millidegrees within 0..359.999 degrees, at most three fractional places; reject exponent/nonfinite/overflow here too. It checks the summary count; it does not recompute physical centroids or rotation from a board.

Version 3 additionally parses the exact populated BOM header, one quantity-1 component row per UUID, optional exact sourcing strings/blank, and existing mounting names. It requires ascending unique UUID BOM order, the identical component-ID set/count as calibrated placement, and exact shared designator/value/footprint/mounting fields per ID. It recomputes only the existing identity digest: ASCII `viamind-populated-components-v1`, zero byte, then sorted 16-byte network-order UUIDs. The digest must match the declared summary. Placement row order remains its existing reference order; membership does not imply equal row order. Empty sets accept header-only files and the correctly empty digest.

Use a bounded original CSV state machine from this spec, not line splitting or code copied from another parser. Reject malformed quotes, wrong field counts, bad UTF-8, trailing junk, duplicate headers/IDs, excess rows/field sizes and invalid values before unbounded field or row allocation. At most 100000 rows per parsed file, 4096 bytes per geometry/reference/value/footprint field, 512 bytes per sourcing field, bounded aggregate parsed text/identity storage from the 32 MiB input cap. Preserve embedded Unicode/quoted CR/LF and doubled quotes exactly. Return only scalar assembly facts and at most fixed file records, never component arrays or file payloads.

## Shared CLI, MCP and desktop observation

Add one strict typed `inspect_output_release` request `{path}` and a headless CLI equivalent with JSON result on stdout. MCP admits this immutable filesystem observation in startup read-only mode. No report/pass flag or project mutation is accepted from callers. Run blocking work without holding document/native/controller locks. Result includes requested absolute root, format/version/profile, historical exported project/cursor, declared/observed file counts and byte lengths, optional calibrated count and matched-set digest/count, explicit checks performed and checks not performed. No single boolean implies manufacturing acceptance.

Desktop adds a separate scrollable read-only inspection form reachable from the existing saved-output area. One fallible named retained worker, disabled bounded path/action while pending, hidden completion retention, clear readable errors and complete historical source disclosure. No overlay, selection, document cursor or current-design state changes. It can inspect a historical release for another project and labels that provenance clearly. At 640×400 path, Inspect and result/limitations remain reachable through scrolling.

## Required verification before review

Independent fixtures cover every supported version/profile, original generated releases and prior bytes, empty population, opposite UUID/reference ordering, Unicode/quotes/multiline fields, exact component digest/count/shared fields and intentionally changed same-length unchecked content honestly passing only declared checks. Missing files, wrong lengths/layout/version/summary, traversal/absolute/duplicate keys, symlinks/subdirectory substitution, FIFO/device metadata/open races, observed same-length mutation/replacement, malformed JSON/CSV, limits and permit cancellation reject without partial result or document/payload mutation.

Actual CLI/initialized two-client MCP parity and read-only admission, full session/native invariance, compact egui input/pending/hidden/error recovery, production native evidence with exact binary identities and durable reports, all four Linux workspace gates, canonical actual-tool reference regeneration/drift, isolated workflow and exact final-head CI are required. Independent review must approve this spec before Rust changes.
