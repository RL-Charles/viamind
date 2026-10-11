# 0002. Polygon booleans and offsets: i_overlay integer API

- Status: proposed
- Date: 2026-10-10
- Ticket: T-0018 (story E2-S2)

## Context

Copper pours, keepouts, DRC copper checks and Gerber output need polygon union,
difference, intersection and offset (clearance inflation). The engine must:

1. **Be robust** on the messy inputs a board produces: thousands of overlapping
   obstacles, touching and collinear edges, holes.
2. **Stay on the nanometre grid.** All model geometry is integer `Nm`; results
   should land on the same grid without an f64 round trip.
3. **Fill a realistic pour fast:** a ~50k-vertex pour in under 200 ms, so
   re-pouring after an edit feels interactive.
4. Be pure Rust under a licence compatible with MIT.

The two candidates are [`geo`](https://crates.io/crates/geo) 0.33.1
(`BooleanOps`, `Buffer`) and [`i_overlay`](https://crates.io/crates/i_overlay)
9.0.1 used directly. Both are MIT/Apache-2.0. Note that `geo` 0.33 itself
delegates its booleans and buffering to `i_overlay` (version 4.5, through its
`f64` API), so this is really "`i_overlay` 9 integer API" vs "`i_overlay` 4 via
`geo`'s float wrapper".

## Benchmark

`crates/geom/benches/boolean.rs` (`cargo bench -p viamind-geom --bench boolean`).
Scene: a 100 × 80 mm board with 1,100 vias (Ø 0.6 mm), 400 tracks (0.2 mm
capsules, 1–10 mm long) and 1,500 rectangular pads, flattened to polygons with
1.5 µm chord tolerance: **3,000 overlapping obstacles, 50,000 vertices**.
Clearance 0.2 mm, round joins. All libraries agree on every result's area
(to ~1e-8 relative) and, for the booleans, on the exact output vertex count.

Release build, Intel Core Ultra 7 255U, single-threaded (no `rayon` features),
criterion medians. The machine was shared with other agents; the numbers below
are from a low-load run (repeat runs varied by ±10 %).

| Operation | Output vertices | i_overlay `i64` | i_overlay `i32` | geo (`f64`) |
| --- | ---: | ---: | ---: | ---: |
| Union of all obstacles | 38,832 | 25.8 ms | 19–23 ms | 22.1 ms |
| Board outline − obstacles | 38,746 | 23.7 ms | **19.3 ms** | 22.1 ms |
| Offset obstacles by 0.2 mm (1.8° joins) | 312k / 285k (geo) | 335 ms | 254 ms | 222 ms |
| Offset, 0.35° joins (i_overlay only) | 1.42M | — | 1,144 ms | not possible¹ |
| Pour = offset, then difference | 311k / 283k (geo) | 430 ms | 322 ms | 300 ms |
| **Pour with obstacles pre-inflated²** (107k input vertices) | 52,032 | 47.3 ms | **38.1 ms** | 42.6 ms |
| Same, through `viamind_geom::boolean::difference` | 52,032 | | 38.3 ms | |

¹ `geo` clamps the round-join step to at least 0.01π (1.8°), about 25 nm of chord
error at a 0.2 mm clearance. `i_overlay` 9 goes down to 1/1024 turn (≈1 nm).

² Each via and track is grown by the clearance *before* flattening (larger
radius / width) and each pad becomes a rounded rectangle, so the pour is a single
difference.

### Findings

- **Booleans are a tie.** The `i32` engine is the fastest (10–15 % ahead of
  `geo`); `i64` is about 20 % slower than `i32`.
- **Offsetting flattened curves is the expensive part**, for both libraries:
  every vertex of a 32-gon via gets its own round join, so 50k input vertices
  become ~300k output vertices and the pour takes 300+ ms. `geo` is ~10 % faster
  here mostly because its coarser minimum join step emits ~9 % fewer vertices.
- **Growing primitives analytically meets the budget with room to spare:**
  38 ms for a pour whose result has 52k vertices, 5× under the 200 ms target.
  Circles and capsules (vias, tracks) are exactly representable as "core +
  radius" (see `viamind_geom::Shape`), so their clearance outline is just a
  bigger shape.

## Decision

Use **`i_overlay` 9 directly, through its integer API**, behind a thin
`viamind_geom::boolean` module (`union`, `intersection`, `difference`,
`offset`, returning `Region { outer, holes }`). Do not depend on `geo`.

- Coordinates are shifted to the centre of the inputs' bounding box and run on
  the **`i32` engine** when the geometry (plus offset) fits in ±2³⁰ nm (≈ ±1.07 m)
  around that centre, which covers every board; otherwise the `i64` engine.
  Results are exact integers on the nm grid; no f64 conversion is involved.
- **Pours inflate primitives analytically** (via/pad radius, track width,
  rounded-rectangle pads) and only fall back to `boolean::offset` for arbitrary
  polygons (custom pads, keepouts, board cut-outs), which have few vertices.
- `geo` stays a dev-dependency of the benchmark only, so the comparison can be
  re-run; it is not used by library code.

Why not `geo`: same engine underneath but an older version (4.5) reached through
an `f64` adapter, so results leave the integer grid and need re-rounding; it
pulls in `geo-types`, a second `rstar` (0.12), `geographiclib-rs`, `rand` and
more; and it cannot produce finer round joins than 1.8°. Its only advantage
(slightly faster offsets) comes from emitting coarser arcs.

## Consequences

- `crates/geom` depends on `i_overlay` 9 (4 small `i_*` crates). The `boolean`
  module is the only place that touches it; swapping engines later means
  rewriting one file.
- The pour fill (story E2-S2 follow-up) must build its obstacle set from
  inflated `Shape`s, not by offsetting already-flattened polygons; a
  `Shape`-level "outline grown by d" helper (rounded rectangles in particular)
  is the next piece of work.
- `allow_multithreading` (rayon) is off. It is worth re-measuring when pours
  reach hundreds of thousands of vertices.
- Re-run `cargo bench -p viamind-geom --bench boolean` when bumping `i_overlay`.
