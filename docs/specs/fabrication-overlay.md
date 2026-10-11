# Bounded fabrication-file overlay (T90)

## Independent public sources

Specification written before implementation. Public references consulted
2026-10-11:

- [Ucamco Gerber specification 2024.05](https://www.ucamco.com/files/downloads/file_en/456/gerber-layer-format-specification-revision-2024-05_en.pdf),
  sections 4.2–4.4, 4.6–4.9, 4.13 and 5.6: metric format, standard apertures,
  linear moves/draws/flashes, transforms, end marker and file attributes.
- [Ucamco XNC specification 2021.11](https://www.ucamco.com/files/downloads/file_en/452/xnc-format-specification-revision-2021-11_en.pdf),
  sections 2.3, 2.6 and 3: explicit decimal metric tool diameters and drill hits,
  aligned with fabrication coordinates.
- [Existing native export behavior](fabrication-gerber.md) and original
  Viamind exporter provide the supported output contract; no external code,
  artwork, parser, application assets or reference-viewer implementation copied.

Gerber aperture rotation is counterclockwise in its upward-Y coordinate frame.
The viewer converts file Y to negative native Y and keeps every side in top view.
Negative solder-mask files represent openings; the UI explicitly displays those
openings, without treating them as positive copper or filling an infinite plane.

## Explicit viewer scope

Read one to six individually named `.gbr` or `.xnc` files. This increment supports
the strict command subset written by Viamind's current two-layer exporter, plus
equivalent bounded spacing/line-ending variations. Files are read independently;
directory traversal, archives, job manifests and automatic companion discovery
are excluded. The viewer never modifies a source or the design and makes no
fabrication, DRC, netlist or geometric-equivalence certification claim.

Gerber accepts exactly metric absolute leading-zero-omitted 5.6 coordinates;
simple circular and rectangular aperture declarations (no holes); Dnn selection;
G01; explicit X/Y and D01/D02/D03; dark objects; no mirror; unit scale; explicit
aperture rotation with at most three decimal degree places; and M02. Rectangular
apertures may flash at any exact millidegree angle. Linear strokes require a
circular aperture and zero rotation. A zero-diameter circular aperture is allowed
only for Profile linear centerlines. No modal omitted coordinate or operation
guessing occurs.

Supported file functions are Copper,L1,Top; Copper,L2,Bot; Soldermask,Top;
Soldermask,Bot; and Profile,NP. Each requires the corresponding positive or
negative file-polarity declaration. Part,Single is recognized; an optional
SameCoordinates UUID is retained. Aperture functions SMDPad,CuDef; ComponentPad; ViaPad; Conductor and
Profile and TD reset are recognized. Duplicate/conflicting headers, undefined or
redefined apertures, premature graphics, missing end marker or trailing commands
reject. Comments remain comments. Unknown semantic commands/attributes reject
the entire request, without publishing a partial picture.
Idempotent repetitions of the supported modal LPD/LMN/LS1/G01/G05 commands are
accepted; unique format/unit/file-identity declarations remain singleton headers.

XNC accepts printable ASCII comments, M48, METRIC, unique T01..T99 positive
decimal diameter declarations, header end %, G05, declared tool selection,
explicit decimal X/Y drill hits and M30. Coordinates and sizes preserve integer
Nm with at most six nonzero decimal places; extra trailing zeroes may be accepted.
Routing, slots, inches, implicit fixed-decimal positions, tool attributes and
unknown commands reject. XNC has no project-identity attribute: the result and
UI explicitly label alignment as the user's chosen common origin.

Limits checked before allocation or filesystem work where applicable:
1–6 paths, each 1–4096 encoded bytes; 4 MiB per regular file and 16 MiB total;
4096 bytes per command/comment; 100000 commands per request; 256 apertures per
Gerber; 10000 aggregate drawable primitives; coordinates within the export's
±99999.999999 mm range; positive aperture sizes at most 10000 mm. Unsupported
input rejects atomically. Safe Linux NONBLOCK opening and descriptor regular-file
checks prevent FIFO races; reads cap at limit+1 and report observed read changes.
One nonwaiting process-wide reader admission prevents overlapping request copies.
Only bounded parsed primitives and source metadata survive reading.

## Shared inspection action

Typed `inspect_fabrication_files` input is `{paths, expected_project,
expected_cursor}`. Exact current project and full cursor are required before
reading. Capture only these identities under the engine mutex, read and parse
outside it, then return the captured provenance plus immutable ordered file
reports and native-frame geometry. Concurrent edits do not invalidate the read's
historical contents; they prevent the desktop from displaying it on a changed
document. Per-file reports include absolute path, exact source SHA256, kind,
file polarity, optional SameCoordinates UUID and primitive counts. A declared
UUID different from the requested project rejects, preventing a misleading
overlay. XNC and Gerber without SameCoordinates have an explicit
unverified-common-origin flag. File identity is not
a claim that bytes still match the current board or remain unchanged afterward.

Result geometry uses exact integer-Nm centers/endpoints/dimensions and exact
millidegree rotation; drawing converts to screen coordinates only at paint time.
No synthetic project objects, IDs, selection changes, history or dirty state.
The same typed behavior is dispatched by MCP and desktop; synchronous filesystem
work runs on a blocking/background worker. Admission is released on every exit.

## Desktop behavior

File → Inspect fabrication files… opens a scrollable form with six explicit file
path fields, Inspect, a source/count/coordinate explanation and independent layer
visibility. Empty fields are omitted; at least one path is required. One retained
fallible background worker disables duplicate Inspect and path edits while busy.
Completion while hidden is retained. Spawn/disconnection/errors preserve fields
and expose a recoverable message. Changing paths clears the previous result.

An explicit Show fabrication overlay checkbox paints selected parsed primitives
over the PCB using the same canvas camera and clipped viewport. Copper, mask
openings, profile centerlines and drills have distinct legend colors. The current
board remains visible for comparison. Local visibility and Close/Hide never
change engine selection/history. Pan/zoom/Home keep the overlay aligned. Full
project/cursor changes clear the displayed overlay, including peer edits and
same-project reopen. The viewer never enables overlay in schematic workspace.

At 640×400, responsive fields and scrolling keep Inspect, errors and result
controls reachable. Closing the form hides its overlay while retaining an active
worker/result for reopening. Only explicit current results can display; failures
never leave a previous overlay masquerading as a new successful inspection.

## Required evidence

Independent original signed/odd-Nm Gerber and XNC fixtures verify exact centers,
dimensions, rotated corners, sides, mask-opening semantics and Y inversion.
Export then parse checks cover all five Gerbers and drill output. Mutated outputs
with a shifted primitive prove the reader observes file contents independently
of the model/exporter. Unsupported commands, malformed state, bounds, duplicate
headers/apertures, arbitrary rotation, EOF/trailing data, file races and permit
release tests reject without session changes. Shared MCP strict schema and
two-client historical-read/stale-overlay behavior, actual egui path/Inspect/
visibility/compact/error/worker tests, readable native normal/compact overlays,
exact production stdio and desktop HTTP, full Linux gates and provenance are
required before review/merge. General Gerber and external CAM compatibility
remain open parent work.
