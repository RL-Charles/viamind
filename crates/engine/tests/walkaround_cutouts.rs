//! Independently derived substrate detours, strict margins and atomic refusal.
use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Pad, Pads, Part, Project, Side, Track, Via};
use viamind_engine::{EngineHandle, Origin, PadEndpointId, RoutePadWalkaround};
use viamind_geom::{Nm, Point, Polygon};
static TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn rectangle(x0: i64, y0: i64, x1: i64, y1: i64) -> Polygon {
    Polygon::new(vec![p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)])
}
fn fixture(cutouts: Vec<Polygon>) -> (Project, [PadEndpointId; 2]) {
    let mut project = Project::new("Substrate detour");
    project.board.outline = Some(BoardOutline::new(rectangle(0, 0, 40_000_000, 20_000_000), cutouts).unwrap());
    let ids = [10_000_000, 30_000_000].map(|x| {
        let pad = Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: Point::ORIGIN,
            width: Nm(1_000_001),
            height: Nm(1_000_001),
            net: Some("N".into()),
        };
        let part = Part {
            id: ObjectId::new(),
            schematic_symbol: None,
            designator: format!("J{x}"),
            value: String::new(),
            footprint: String::new(),
            position: p(x, 10_000_000),
            rotation_mdeg: 0,
            side: Side::Top,
            pads: Some(Pads::try_from(vec![pad.clone()]).unwrap()),
        };
        let endpoint = PadEndpointId { part_id: part.id, pad_id: pad.id };
        project.board.parts.insert(part.id, part);
        endpoint
    });
    (project, ids)
}
fn input(e: &EngineHandle, ids: [PadEndpointId; 2]) -> RoutePadWalkaround {
    RoutePadWalkaround {
        expected_project: e.read(|p| p.id),
        expected_cursor: e.event_cursor(),
        start: ids[0],
        target: ids[1],
        grid: Nm(1_000_000),
        width: Nm(250_001),
        rules: Default::default(),
    }
}
fn reject(e: &EngineHandle, cmd: RoutePadWalkaround) {
    let before = e.session_state();
    assert!(e.plan_pad_walkaround(&cmd).is_err());
    assert!(e.execute_typed(cmd, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
}
#[test]
fn exact_cutout_detours_ignore_winding_net_side_and_copper_clearance() {
    let _guard = TEST.lock().unwrap();
    for (side, reverse) in [(Side::Top, false), (Side::Bottom, true)] {
        let mut cutout = rectangle(19_000_000, 8_000_000, 21_000_000, 12_000_000);
        if reverse {
            cutout.points.reverse();
            cutout.points.rotate_left(1);
        }
        let (mut project, ids) = fixture(vec![cutout]);
        for part in project.board.parts.values_mut() {
            part.side = side;
            part.rotation_mdeg = 90_000;
        }
        let e = EngineHandle::new(project);
        let cmd = input(&e, ids);
        let before = e.session_state();
        let plan = e.plan_pad_walkaround(&cmd).unwrap();
        assert_eq!(e.session_state(), before);
        let expected =
            vec![p(10_000_000, 10_000_000), p(10_000_000, 7_000_000), p(22_000_000, 7_000_000), p(22_000_000, 10_000_000), p(30_000_000, 10_000_000)];
        assert_eq!(plan.plan.path.points, expected);
        assert!(plan.complete_scoped_preflight);
        assert_eq!(plan.plan.work.obstacle_boxes, 1);
        let layer = if side == Side::Top { CopperLayer::Top } else { CopperLayer::Bottom };
        assert_eq!(plan.layer, layer);
        let mut only_copper = cmd.clone();
        only_copper.rules.copper_clearance_mm = 3.;
        assert_eq!(e.plan_pad_walkaround(&only_copper).unwrap().plan.path.points, expected);
        let output = e.execute("route_pad_walkaround", serde_json::to_value(cmd).unwrap(), Origin::User).unwrap();
        let after = e.session_state();
        assert_eq!(after.cursor.sequence, before.cursor.sequence + 1);
        assert_eq!(output.created.len(), 4);
        for (id, points) in output.created.iter().zip(expected.windows(2)) {
            let track = &after.project.board.tracks[id];
            assert_eq!((track.start, track.end, track.layer, track.net.as_deref()), (points[0], points[1], layer, Some("N")));
        }
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        e.redo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), after.project);
    }
    // Odd width ceil(250001/2)=125001; +874998 edge +1 =1000000.
    // Closed contact at y7000000 is blocked, so the next strict grid escape is y6000000.
    let (project, ids) = fixture(vec![rectangle(19_000_000, 8_000_000, 21_000_000, 12_000_000)]);
    let e = EngineHandle::new(project);
    for (edge, escape) in [(0.874997, 7_000_000), (0.874998, 6_000_000), (1.250001, 6_000_000)] {
        let mut cmd = input(&e, ids);
        cmd.rules.edge_clearance_mm = edge;
        let plan = e.plan_pad_walkaround(&cmd).unwrap();
        assert_eq!(plan.plan.path.points[1], p(10_000_000, escape));
    }
    // Signed exact geometry with quarter-turn/mirrored, nonzero-local off-grid centers.
    let (mut project, ids) = fixture(vec![rectangle(19_000_000, 8_000_000, 21_000_000, 12_000_000)]);
    let outline = project.board.outline.as_mut().unwrap();
    for ring in std::iter::once(&mut outline.outer).chain(outline.cutouts.iter_mut()) {
        for point in &mut ring.points {
            point.x.0 -= 40_000_000;
            point.y.0 -= 20_000_000;
        }
    }
    for part in project.board.parts.values_mut() {
        part.position.x.0 -= 40_000_000;
        part.position.y.0 -= 20_000_000;
        part.side = Side::Bottom;
        part.rotation_mdeg = 90_000;
        part.pads.as_mut().unwrap().0[0].position = p(123, 129);
    }
    let e = EngineHandle::new(project);
    let cmd = input(&e, ids);
    let before = e.session_state();
    let plan = e.plan_pad_walkaround(&cmd).unwrap();
    assert_eq!(
        plan.plan.path.points,
        vec![
            p(-29_999_871, -9_999_877),
            p(-30_000_000, -10_000_000),
            p(-30_000_000, -13_000_000),
            p(-18_000_000, -13_000_000),
            p(-18_000_000, -10_000_000),
            p(-10_000_000, -10_000_000),
            p(-9_999_871, -9_999_877)
        ]
    );
    assert_eq!(plan.plan.path.endpoint_stub_segments, 2);
    assert_eq!(e.session_state(), before);
    e.execute_typed(cmd, Origin::User).unwrap();
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    let (project, ids) =
        fixture(vec![rectangle(14_000_000, 8_000_000, 16_000_000, 12_000_000), rectangle(24_000_000, 8_000_000, 26_000_000, 12_000_000)]);
    let e = EngineHandle::new(project);
    let plan = e.plan_pad_walkaround(&input(&e, ids)).unwrap();
    assert_eq!(plan.plan.work.obstacle_boxes, 2);
    assert_eq!(plan.plan.path.points, vec![p(10_000_000, 10_000_000), p(10_000_000, 7_000_000), p(30_000_000, 7_000_000), p(30_000_000, 10_000_000)]);
}
#[test]
fn cutout_refusals_and_combined_obstacle_budget_preserve_complete_session() {
    let _guard = TEST.lock().unwrap();
    for variant in 0..7 {
        let cutout = match variant {
            0 => Polygon::new(vec![p(19_000_000, 8_000_000), p(21_000_000, 8_000_000), p(20_000_000, 12_000_000)]),
            1 => Polygon::new(vec![p(20_000_000, 8_000_000), p(22_000_000, 10_000_000), p(20_000_000, 12_000_000), p(18_000_000, 10_000_000)]),
            2 => Polygon::new(vec![
                p(19_000_000, 8_000_000),
                p(21_000_000, 8_000_000),
                p(21_000_000, 10_000_000),
                p(20_000_000, 10_000_000),
                p(20_000_000, 12_000_000),
                p(19_000_000, 12_000_000),
            ]),
            // Every exact endpoint/stub must remain outside the inflated substrate exclusion.
            3 => rectangle(9_900_000, 9_900_000, 10_100_000, 10_100_000),
            // Gap to outer edge is narrower than route width + both edge clearances.
            4 => rectangle(19_000_000, 1, 21_000_000, 19_999_999),
            5 => rectangle(10_200_000, 10_200_000, 10_300_000, 10_300_000),
            _ => rectangle(19_000_000, 8_000_000, 21_000_000, 12_000_000),
        };
        let (mut project, ids) = fixture(vec![cutout]);
        if variant == 5 {
            project.board.parts.get_mut(&ids[0].part_id).unwrap().position = p(10_490_000, 10_490_000);
        }
        // Corrupt after normal construction: inherited outline validation must still refuse.
        if variant == 6 {
            project.board.outline.as_mut().unwrap().cutouts[0].points[0] = p(0, 8_000_000);
        }
        let e = EngineHandle::new(project);
        let mut cmd = input(&e, ids);
        if variant == 5 {
            cmd.rules.edge_clearance_mm = 0.;
        }
        reject(&e, cmd);
    }
    //63 cutouts +1 foreign track exactly fill the shared64 budget. Adding a SAME-NET
    //via contributes a drill box too, and must refuse instead of granting a second budget.
    let cutouts = (0..63).map(|i| rectangle(1_000_000 + i * 300_000, 1_000_000, 1_100_000 + i * 300_000, 1_100_000)).collect();
    let (mut project, ids) = fixture(cutouts);
    let track = Track {
        id: ObjectId::new(),
        start: p(25_000_000, 8_000_000),
        end: p(25_000_000, 12_000_000),
        width: Nm(250_001),
        layer: CopperLayer::Top,
        net: Some("FOREIGN".into()),
    };
    project.board.tracks.insert(track.id, track);
    let e = EngineHandle::new(project);
    let plan = e.plan_pad_walkaround(&input(&e, ids)).unwrap();
    assert_eq!(plan.plan.work.obstacle_boxes, 64);
    e.execute_typed(input(&e, ids), Origin::User).unwrap();
    e.undo(Origin::User).unwrap();
    let mut project = e.read(Clone::clone);
    let via = Via { id: ObjectId::new(), position: p(30_000_000, 5_000_000), diameter: Nm(600_000), drill: Nm(300_000), net: Some("N".into()) };
    project.board.vias.insert(via.id, via);
    let e = EngineHandle::new(project);
    let before = e.session_state();
    let error = e.plan_pad_walkaround(&input(&e, ids)).unwrap_err().to_string();
    assert!(error.contains("obstacle box limit64"), "{error}");
    reject(&e, input(&e, ids));
    assert_eq!(e.session_state(), before);
}
#[test]
fn real_outline_edits_and_reopen_invalidate_checked_cutout_intents() {
    let _guard = TEST.lock().unwrap();
    let (project, ids) = fixture(vec![rectangle(19_000_000, 8_000_000, 21_000_000, 12_000_000)]);
    let e = EngineHandle::new(project);
    let cmd = input(&e, ids);
    let plan = e.plan_pad_walkaround(&cmd).unwrap();
    let outline = e.read(|p| p.board.outline.clone());
    e.execute("set_board_outline",serde_json::json!({"expected":{"project_id":cmd.expected_project,"outline":outline},"outline":{"outer":[{"x_mm":0.,"y_mm":0.},{"x_mm":40.,"y_mm":0.},{"x_mm":40.,"y_mm":20.},{"x_mm":0.,"y_mm":20.}],"cutouts":[[{"x_mm":19.,"y_mm":6.},{"x_mm":21.,"y_mm":6.},{"x_mm":21.,"y_mm":14.},{"x_mm":19.,"y_mm":14.}]]}}),Origin::agent("peer")).unwrap();
    reject(&e, cmd);
    let fresh = input(&e, ids);
    let fresh_plan = e.plan_pad_walkaround(&fresh).unwrap();
    assert_ne!(fresh_plan.plan.path.points, plan.plan.path.points);
    let file = tempfile::NamedTempFile::new().unwrap();
    e.read(|p| viamind_doc::file::save(p, file.path())).unwrap();
    e.open_project(
        viamind_engine::OpenProject { path: file.path().to_path_buf(), discard_dirty: true, expected_cursor: Some(fresh.expected_cursor) },
        Origin::User,
    )
    .unwrap();
    reject(&e, fresh);
}
