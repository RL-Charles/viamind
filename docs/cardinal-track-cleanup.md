# Checked cardinal track cleanup

Select stored tracks on the PCB canvas (Shift adds to selection), then choose
**Board → Clean cardinal track joints**. Alternatively choose **Named net**
and an optional outer layer in the dialog. Preview does not change the document,
history, selection or native file. Green lines show proposed surviving tracks;
yellow rings mark redundant stored joints. Review actual scoped DRC coverage,
excluded parts, unperformed checks and retained/unsupported counters. **Apply**
recomputes against the captured full session cursor, commits all replacements
atomically, and creates one undo. Cancel/Escape discards the local draft. Peer
edits or selection changes invalidate the preview; preview again before applying.

Agents use `plan_cardinal_track_cleanup` and `cleanup_cardinal_tracks` with the
same input:

```json
{
  "expected_project": "project UUID from get_session_state",
  "expected_cursor": {"stream_id": "stream UUID", "sequence": 12},
  "tracks": {"source": "net", "net": "N", "layer": "top"}
}
```

Use `{"source":"ids","ids":["stored track UUID", "another track UUID"]}`
to select explicit tracks. Omit `rules` for the existing scoped defaults, or
supply the shared DRC rules. `layer: null` selects both outer layers. The named
net must match exactly; model selection is never an implicit server selector.

## Supported geometry and limits

Only exact horizontal/vertical straight chains with the same assigned net,
outer side and integer-Nm width merge. The two far endpoints must lie strictly
opposite the removed joint. Existing branches (including unselected neighbors),
all exact quarter-turn explicit pad centers and through-via centers remain
anchors. The smallest original track UUID survives. Reversals, overlaps,
diagonals, unassigned tracks and duplicate final tracks are retained.

Every chain touching the closed centerline AABB of **any** via drill hole is
retained conservatively, regardless of its net. This preserves the current
conservative stored-endpoint guide proof; this command does not change that
proof or certify physical copper continuity. It does not optimize bends, move
existing copper, perform shove or remove overlaps.

Borrowed geometry/text limits precede a board-only, sourcing-free snapshot:
100000 structural objects, 512-byte indexed labels, 2MiB captured text,
2048 selected tracks, 1024 deleted segments and 1M visited hole candidates.
Two operations can be retained concurrently; excess work refuses immediately.
Unrelated schematics and assembly metadata are not copied. Unsupported explicit
pad transforms and malformed geometry refuse the whole request. A complete
shared joint scoped DRC report is required; affected violations or incomplete/
truncated checks refuse without mutation. Unrelated violations and excluded
legacy previews can remain and are reported explicitly. This is bounded
synchronous commit preflight, not a latency guarantee or complete manufacturing
DRC. Preview runs in a retained desktop worker and stale results cannot revive a
cancelled or replaced draft.

The exact behavioral specification is [here](specs/cardinal-track-cleanup.md).

[Verification and native provenance](cardinal-track-cleanup-verification.md)
records Linux gates, actual HTTP checks, compact input proof and readable native preview.
