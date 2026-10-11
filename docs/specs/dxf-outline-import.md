# Straight-edge DXF board outline import

Clean-room behavioural specification, written before implementation. Sources:
[official DXF reference](https://images.autodesk.com/adsk/files/autocad_2014_pdf_dxf_reference_enu.pdf),
[LWPOLYLINE](https://help.autodesk.com/cloudhelp/2017/ENU/AutoCAD-DXF/files/GUID-748FC305-F3F2-4F74-825A-61F04D757A50.htm),
[LINE](https://help.autodesk.com/cloudhelp/2025/ENU/AutoCAD-DXF/files/GUID-FCEF5726-53AE-4C43-B4EA-C84EB8686A66.htm),
[VERTEX](https://help.autodesk.com/cloudhelp/2024/ENU/AutoCAD-DXF/files/GUID-0741E831-599E-4CBF-91E1-8ADBCFD6556D.htm), and
[header units](https://help.autodesk.com/cloudhelp/2020/ENU/AutoCAD-DXF/files/GUID-A85E8E67-27CD-4C59-BE61-4DC9FADBE74A.htm).
Only the published format, never proprietary implementation or assets, is used.

## Input and limits

Accept textual ASCII DXF group-code/value pairs with well-formed sections and EOF.
A single ENTITIES section is required. Binary DXF, truncated pairs/sections and
repeated scalar geometry fields fail. Comments and non-ENTITIES sections are ignored.
Limit input to 8 MiB and geometry to the existing board outline limits (2048 vertices,
64 cutouts, coordinates within +/-10000 mm). No external resources are loaded.

Units are explicit: default `header` requires a supported nonzero $INSUNITS
(inches, mm, cm, metres or mils). Explicit `mm` or `inches` intentionally overrides
the header, allowing legacy unitless files. Values must be finite and within range
before conversion to integer Nm, rounded to the nearest nanometre. No heuristic
scaling, welding tolerance, rotation or origin shift is applied. DXF +Y points
up; convert it to native -Y so the imported board preserves its visual orientation
in Viamind (whose canvas +Y points down). X is unchanged, with no origin shift.

An optional exact layer name selects entities on that layer (default layer is 0).
POLYLINE selection uses its parent layer; child VERTEX records inherit that
selection regardless of redundant child layer tags. Without a filter every
entity must be supported. Unsupported entities on a selected
layer fail with their type and an actionable instruction; never silently drop curves.

## Geometry

Support closed, straight 2D LWPOLYLINE (closed flag, declared vertex count checked),
old-style POLYLINE/VERTEX/SEQEND, and LINE entities assembled into loops using exact
Nm endpoint equality. POLYLINE curve/spline/3D/mesh flags, nonzero bulge, line width,
elevation/thickness, nonstandard extrusion or paper-space geometry fail. ARC,
CIRCLE, SPLINE, INSERT and other entities require conversion to supported straight
closed polylines in the source drawing. Curved boundaries are a later increment.

Every LINE endpoint must have degree two. Open, branching, duplicate/degenerate
or self-intersecting chains fail. Closed polyline duplicate final vertices are
normalized away. Multiple rings form one board: the largest absolute-area ring is
the outer boundary and all remaining rings are cutouts. Existing BoardOutline
validation rejects intersections, touching, disjoint boards and nested holes.
Winding and input ordering do not affect classification.

## Shared operation and desktop

`import_board_outline_dxf` receives text, units, optional layer and required exact
ExpectedOutline guard. Parse and validate before mutation, then replace the outline
with one ordinary BoardOutline delta. Failure leaves model, history and dirty state
unchanged. Reimporting identical geometry is a no-op. Undo/redo, replay, save and
MCP schema discovery reuse the existing engine paths.

The existing Board outline dialog exposes an ASCII DXF path, units selector and
optional layer filter. Import reads a bounded local text file and invokes the same
catalog action/engine command. Failure retains the dialog and shows the reason;
success closes it. Reading and parsing never mutate the model directly.
