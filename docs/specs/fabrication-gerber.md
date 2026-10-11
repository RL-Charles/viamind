# Scoped outer-copper fabrication export

Independent behavior, specified before implementation for T-0051. Public sources:

- [Ucamco Gerber specification 2026.05](https://www.ucamco.com/files/downloads/file_en/554/gerber-layer-format-specification-revision-2026-05_en.pdf), sections 4.2, 4.3, 4.8, 4.9, 5.6 and 6: metric Cartesian coordinates viewed from above, positive CCW rotation, flashed pads, stroked conductors, common alignment, file functions/polarity and separate closed profile contours.
- [Ucamco XNC specification 2021.11](https://www.ucamco.com/files/downloads/file_en/452/xnc-format-specification-revision-2021-11_en.pdf), sections 2–3: documented Excellon/IPC-NC-349 subset, decimal metric coordinates, finished hole diameters, M48/METRIC/tool declarations/%/G05/tool selection/hits/M30.
- [Gerbonara reader documentation](https://gerbolyze.gitlab.io/gerbonara/): independent Gerber/Excellon reader and SVG rendering API used for verification, not implementation.

## Supported increment and refusals

Export a complete immutable project snapshot into a new directory containing top/bottom copper, top/bottom mask openings, a profile and plated through-via NC drill. Require a persistent polygon outline; close outer and cutout rings exactly. Require explicit pad lists for every part; an explicitly empty list is supported. Legacy graphical footprint previews have no electrical manufacturing geometry and reject export with the component identified.

Only rectangular SMD pads, straight top/bottom tracks and circular through vias are represented. Inner copper, pours, arcs, blind/buried/microvias, component through holes, slots, silkscreen, paste, panelization, X3 assembly and material/finish/stackup instructions are unsupported. No sourcing or fabrication-readiness claims. Existing pad-only DRC or connection guides do not certify this package. Full S-071 acceptance including a fab online checker stays open.

## Coordinates and images

All files retain the model origin, use X right and Y up (`export_x=model_x`, `export_y=-model_y`) and are viewed from above, including bottom layers. Never mirror an entire bottom file. Gerber uses absolute metric 5.6 format (1 nm resolution), with explicit coordinates/operations; reject any output coordinate outside ±99999.999999 mm. Sizes use exact decimal-mm integer formatting. Model geometry remains integer nm; arbitrary pad-center rotation is computed at the I/O boundary and rounded once to the nearest nm.

Place each pad center using the shared Part local-X mirror followed by stored CCW rotation and translation. A centered rectangular aperture is invariant under local-X reflection; flash it at the transformed center with Gerber LR equal to the stored positive CCW millidegrees. Reset rotation before conductors. Independently verify an asymmetric noncentral bottom pad at 30° against transformed corners and a second differently placed/rotated pad, so wrong rotation sign, whole-layer mirroring or offset cannot hide in a symmetric fixture.

Copper pads and vias are flashes with SMDPad,CuDef/ViaPad aperture functions. Via copper flashes are full discs; the aligned separate finished-hole NC file removes their centers. Tracks stroke circular apertures for round-ended width. Profile consists of connected closed paths with zero-size profile aperture, including cutouts. Every Gerber declares file function, file polarity and same coordinates. Copper is positive. Mask files contain openings and declare negative file polarity while graphical objects are dark. Pads and untented vias have mask openings; tracks do not. Mask expansion is an explicit export parameter, default 0 mm, finite in 0–10 mm, applied equally to both sides and recorded in the manifest. No mask-rule inference.

Drill uses decimal metric XNC commands, grouping by exact finished drill diameter and stable tool order. XNC tool numbers are exactly two digits, 01–99; reject more than 99 distinct drill diameters before publication. A board without through vias has an empty drill body. Its coordinates align with Gerber. Only plated through-via holes exist in this increment. No unverified controller dialect or machine feeds/speeds. The manifest records supported counts, origin/axes, units, expansion, filenames and exclusions.

## Publication and access

Validate all supported geometry and bounded object/output counts before publication. Limit exported primitives to 200000 and rendered bytes to 128 MiB. Render deterministic bytes with stable ordering and no timestamps. Stage files in a new sibling temporary directory, sync each file and stage, then publish with Linux atomic no-replace directory rename. Existing files/directories/symlinks always reject; there is no overwrite mode. Refuse the active native project path explicitly. Parent directory must exist. Failures before publication leave no final output; OS process/crash interruption may leave an identifiable staging directory. Atomic visibility is promised, not power-loss durability.

CLI `--export-gerber --open PATH --output NEW_DIRECTORY [--mask-expansion-mm N]` exits without a window. MCP `export_fabrication` uses the same snapshot writer and optional expected_cursor; returns source cursor and manifest/path. Desktop Board export form uses the same writer through a catalog action, performs I/O off the UI thread and retains input/errors. Export never changes document/history/dirty state, and reports the exact snapshot cursor rather than claiming later edits are included.

## Verification

Exact deterministic output, rejection without partial publication, collision/race tests and active native protection. CLI invocation and MCP stale/snapshot behavior, accessible desktop form. Independent Gerbonara parsing/rendering validates asymmetric bottom-pad placement and rotation, track widths, via copper/drill alignment, mask dimensions and both outline rings. Linux fmt, strict all-target clippy, workspace tests/build before submission.
