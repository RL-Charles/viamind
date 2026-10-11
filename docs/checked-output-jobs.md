# Checked manufacturing output jobs

Use a saved checked job to require complete, passing scoped DRC before publishing
a NEW manufacturing release directory. The check and every export use the same
immutable board snapshot. Existing generic jobs retain their previous behavior.

A checked descriptor uses `profile: "checked_outer_2layer"` and requires all these
explicit request baselines:

```json
{
  "format": "viamind-output-job",
  "version": 1,
  "name": "Checked manufacturing release",
  "profile": "checked_outer_2layer",
  "mask_expansion_mm": 0.0,
  "checks": {
    "copper_clearance_mm": 0.2,
    "edge_clearance_mm": 0.25,
    "minimum_via_drill_mm": 0.2,
    "minimum_annular_width_mm": 0.1,
    "hole_clearance_mm": 0.25,
    "hole_to_copper_clearance_mm": 0.2,
    "max_violations": 1000
  }
}
```

Choose baselines for the actual manufacturing process. All distances must be
finite from 0 to 100 mm; the violation result cap must be 1 to 10000. Saved settings
remain detached from the native project. This increment adds request baselines; it
does not add persisted named-net policies. Future document policies must be
enforced by the same central DRC on the retained snapshot. Save them through `save_output_job` or
**File → Saved output jobs**, selecting **Checked outer 2-layer**. Run reads the
saved descriptor, including external edits; unsaved form edits are identified.

Headless execution uses the same production path:

```sh
viamind --run-output-job checked.json --open board.viamind.json --output NEW_RELEASE
```

MCP exposes the existing typed load/save/run tools. `run_output_job` accepts the
saved job path, NEW release path and optional full `expected_cursor`; it accepts
no authorization report or pass flag. Desktop work remains in one retained
background worker, with bounded controls and scrollable results.

Successful checked jobs contain twelve fixed files, including
`validation/drc.json` with the full captured source cursor, requested rules,
covered geometry counts, complete scoped DRC report and remaining exclusions.
Generic assembly and supported Gerber/XNC bytes are unchanged. Complete release
size remains bounded to 192 MiB, including at most 16 MiB of validation metadata.
NEW destinations preserve existing files, directories, symlinks and native files.
A refusal or ordinary staging failure publishes no final release. Atomic visibility
is provided; process interruption can leave a named staging directory.

Checked jobs refuse violations, missing explicit outlines, unsupported/legacy
components, incomplete or truncated reports, and any mismatch between checked
and exported pads, tracks, through vias or drill holes. Existing same-net clearance
exemptions and conservative unassigned-copper behavior remain. Static exclusions
include connectivity, drill-to-edge, mask, silk and complete manufacturing checks;
a passing checked release is not full DFM or manufacturer acceptance. Assembly
placement remains generic component origins, with no sourcing/population filtering
or manufacturer centroid/orientation certification.
