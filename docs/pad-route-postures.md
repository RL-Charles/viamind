# Pad routing corners and posture

Open **Board → Route pad connection**, then choose **Cardinal corners** or
**45-degree corners**. Enter the grid and track width and choose **Start route**.
Click two distinct same-net pads on the same copper side, or pick their visible
connection guide. Only explicit rectangular pads with quarter-turn component
rotation can supply exact endpoints.

The preview preserves each exact pad center. A short stub joins an off-grid
center to its nearest grid point; these endpoint stubs may have arbitrary angles.
Confirmed waypoints and the path between grid points use the chosen corner mode.

Use the **Posture** button to change the path locally:

| Corner mode | Primary posture | Alternate posture |
| --- | --- | --- |
| Cardinal corners | Horizontal first | Vertical first |
| 45-degree corners | Straight first | Diagonal first |

Click empty board space to add a snapped waypoint. **Backspace** removes the last
confirmed waypoint. **Escape** cancels the local draft; **Enter** commits the
whole path in one undo step. Changing posture, adding waypoints and cancelling
do not change the design. A concurrent edit makes the frozen proposal stale;
cancel and start a fresh route before retrying.

The commit checks the generated path, including its endpoint stubs, with the
existing bounded scoped clearance and outline preflight. A failed or incomplete
check rejects the entire path. Existing unrelated violations do not make this a
whole-board DRC certificate. See [interactive routing limits](specs/interactive-polyline-routing.md).

Agents use the same `route_pad_posture` command. Supply exact endpoint IDs,
project and event cursor guards, integer nanometre `grid` and `width`,
`mode: "cardinal" | "octilinear"`, and
`posture: "primary" | "alternate"`. Supplied waypoints must already be aligned
to the grid; the desktop snaps pointer positions before adding them.

At most 510 confirmed waypoints and 1024 generated segments are supported.
Coordinates stay within ±10000 mm; the grid is positive and at most 10000 mm.
An out-of-range nearest grid anchor is rejected. Consecutive duplicate points
are removed; nonconsecutive revisits remain.

This planner does not avoid obstacles, push or shove copper, infer pad-edge
attachment, or combine postures with via transitions. Manual segments and the
separate through-via route remain available. Connection guides are independent
conservative guidance, rather than a route completion certificate.
