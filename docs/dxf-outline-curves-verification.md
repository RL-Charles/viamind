# Certified curved DXF outline verification

T-0128 implements the independently approved [public contract](specs/dxf-outline-curves.md)
(approved at `8de85b137b8b37a115e5629416ab43ebe23bee9c`; `43ad48dc` only corrected a
documentation link). Native BoardOutline, native schema and event-log versions are
unchanged; the result is a validated **polygon approximation**. Analytic native curves,
editable native rounded slots and fabrication equivalence remain open S-050 acceptance.

## What changed

- `viamind_doc::dxf_curves` — bounded certified conversion of closed polylines with
  `0 < |bulge| <= 1`, ARC+LINE loops and CIRCLE rings. The straight-only
  `import_board_outline_dxf` command and its bytes are unchanged; straight sources sent
  through the new entry reproduce the predecessor polygon byte for byte (tested on all four
  original fixtures and through the engine).
- `preview_board_outline_dxf` (observation) and `apply_board_outline_dxf` (writer) share one
  engine implementation, one UI action each and one MCP tool each in all agent modes
  (Apply is refused at the protocol in read-only and approval modes).
- The existing outline dialog gains explicit tolerance, Preview/Apply/Reset, report rows and
  a viewport-bounded scroll area; one retained worker holds a weak engine reference.

## Certificate

Every arithmetic step uses Rust's correctly rounded `+ - * / sqrt` and fused `mul_add`,
widened outward (TwoSum / FMA exactness checks keep exact values exact). Source tokens are
exact bounded decimals (32 significant digits, exponent ±32, 64 bytes) converted with exact
integer unit factors. ARC/CIRCLE angles are reduced exactly in fixed-point degrees to a
quadrant and to at most 45°, then a 12-term Taylor series with an enclosed alternating
remainder gives certified sin/cos; axis angles are exact. Bulge arcs bisect algebraically:
midpoint `Q + b/2 (dy, -dx)`, half bulge `b / (1 + sqrt(1 + b²))`. Each emitted chord is
accepted only when `ceil(|b| c / 2 + max endpoint displacement) <= tolerance`, where the
displacement covers the source/evaluation enclosure plus integer-Nm rounding (triangle
inequality over the whole subarc and chord). Reported bounds are these upper bounds, not
measured error. Decision log: quadrant split applies to angle-parametrized ARC/CIRCLE;
bulges bisect from their own endpoints with conservative bbox ± sagitta extrema and a
radius/center domain check that rejects ill-conditioned tiny bulges.

## Tests

- Doc: 8 analytic tests (semicircle ±1 with an analytic 64-chord depth oracle, rational
  bulge ±0.5 exact centre/radius/midpoint, quarter ARC + angle wrap equivalence, asymmetric
  odd-Nm circle cutout with exact quadrant points and Y orientation, rounded-slot area,
  reorder/reverse invariance, every admission budget at boundary and max+1, 23 refusal
  classes) plus 3 numeric unit tests. An independent sampled two-sided arc/chord oracle must
  never exceed the certificate.
- Engine: preview immutability/determinism, digest/stream/outline/project/tolerance guards,
  one delta, no-op, native save, log replay, per-origin undo; path sources, source rewrite,
  peer edit; legacy byte equivalence; same-descriptor FIFO swap refusal without waiting;
  duplicate/unknown/oversized raw input refusal before typed decoding.
- MCP: two actual HTTP clients, text/path parity, stale peer, consumed preview, peer undo
  isolation, ReadOnly/Approval protocol refusal, creator undo.
- Desktop: actual 640x400 input (reveal + effective-clip assertion before every click/type),
  stale/changed-source/peer refusal, Apply, Close/reopen preserving the form, Undo; retained
  worker unit tests for newer generation, closed form, new stream, disconnect, failed spawn,
  busy duplicate start/reset and dropped design. The pre-existing outline UI test now
  reveals targets first because the form scrolls (its first failure is retained).

## Retained verification (local, Linux)

Artifacts: `~/.cache/viamind/native-artifacts/t0128/`.

| Gate | Result | Receipt |
| --- | --- | --- |
| fmt, strict workspace all-target Clippy, workspace tests, workspace build | PASS, **1051 passed / 0 failed / 1 existing ignored** at `fb22b0d6` | `full2.log` (first attempt `full1-clippy-fail.log`: one collapsible-match lint, fixed) |
| Retouched app build | `viamind-fb22b0d6` SHA256 `c9be3c37964ff4edaa8592dc72713c8a38cf737f0ac3d1f60e16afa7bfa0826c` | `full2.log` |
| Catalog / workflow | 122 tools regenerated and `--check` exact; 18 workflow checks; reference unittests OK | `docs/generated/` |
| Production stdio | 24 checks PASS ([stdio.json](verification/t0128/stdio.json)); first run retained failed on a probe KeyError (absent outline field) | `stdio1/`, `stdio2/` |
| Restricted stdio (`--agent-read-only`) | 5 checks PASS ([restricted-stdio.json](verification/t0128/restricted-stdio.json)); two earlier probe-expectation failures retained | `stdio-restricted{1,2,3}/` |
| GUI HTTP, two initialized clients | 29 checks PASS against owned PID 217333 on port 7965 ([http.json](verification/t0128/http.json)) | `http1/` |

The GUI was launched with `cua-driver launch_app` through the owned wrapper
`launch-production-fb22b0d6.sh` (owned fixture "T128 curved DXF owned inspection");
[production-window.png](verification/t0128/production-window.png) is a compositor capture for
launch provenance only (AT-SPI unavailable; no input was sent).

### Native WGPU captures

[compact.png](verification/t0128/compact.png) (640x400, fresh preview with reachable
Preview/Apply/Cancel and report) and [wide.png](verification/t0128/wide.png) (1600x1100, applied
rounded board with circle and slot cutouts and the success notice) come from the
inspection-only `dxf_curves_inspection` example (helper `dxf-inspection-r6`, SHA256
`df1b6277cbd96fb9490924b6983d030d2caaa9c484e117bf9226e593527050ce`). Preparation uses actual egui
Harness input; the same app state and egui memory move into a fixed-size eframe WGPU window
that asserts the full session and document status every pass, requires every control label
and notice fully inside its effective clip for three consecutive passes, reads back one
Screenshot event (original P7 RGBA and P6 RGB retained; PNGs are pixel-identical,
`compare -metric AE` 0) and self-closes. This is not an OS-input or cadence claim.

Earlier helper revisions (r1–r5 refused before capture; diagnostic captures `diag*.png` show
the missing modal) are retained: the transferred egui memory carries the Harness clock's modal fade-in timestamp, so the modal stayed transparent
until the native clock passed it. The final helper waits for that time explicitly; no product
layout changed for this.

## Remaining

Analytic native curves, editable rounded slots, exact curved Gerber output and certified
fabrication equivalence stay open (S-050). Independent review, exact-head CI and human
sign-off are required before merge.
