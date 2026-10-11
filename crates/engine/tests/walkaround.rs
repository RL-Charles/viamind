//! Independently expected detours and atomic shared-query/commit invariants.
use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Pad, Pads, Part, Project, Side, Track, Via};
use viamind_engine::{Command, EngineHandle, Origin, PadEndpointId, RoutePadWalkaround};
use viamind_geom::{Nm, Point, Polygon};
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn fixture() -> (Project, [PadEndpointId; 2]) {
    let mut project = Project::new("Static detour");
    project.board.outline =
        Some(BoardOutline::new(Polygon::new(vec![p(0, 0), p(40_000_000, 0), p(40_000_000, 20_000_000), p(0, 20_000_000)]), vec![]).unwrap());
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
        let id = PadEndpointId { part_id: part.id, pad_id: pad.id };
        project.board.parts.insert(part.id, part);
        id
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
fn barrier(project: &mut Project, net: Option<&str>, layer: CopperLayer) {
    let t = Track {
        id: ObjectId::new(),
        start: p(20_000_000, 8_000_000),
        end: p(20_000_000, 12_000_000),
        width: Nm(250_001),
        layer,
        net: net.map(str::to_owned),
    };
    project.board.tracks.insert(t.id, t);
}
#[test]
fn immutable_checked_plans_atomic_commits_and_conservative_obstacle_coverage() {
    for (typed, side) in [(true, Side::Top), (false, Side::Bottom)] {
        let layer = if side == Side::Top { CopperLayer::Top } else { CopperLayer::Bottom };
        let (mut project, ids) = fixture();
        for part in project.board.parts.values_mut() {
            part.side = side;
        }
        barrier(&mut project, Some("FOREIGN"), layer);
        let e = EngineHandle::new(project);
        let cmd = input(&e, ids);
        let before = e.session_state();
        let plan = e.plan_pad_walkaround(&cmd).unwrap();
        assert_eq!(e.session_state(), before);
        assert!(plan.complete_scoped_preflight);
        assert_eq!(
            plan.plan.path.points,
            [p(10_000_000, 10_000_000), p(10_000_000, 7_000_000), p(21_000_000, 7_000_000), p(21_000_000, 10_000_000), p(30_000_000, 10_000_000)]
        );
        assert_eq!(plan.plan.work.obstacle_boxes, 1);
        let out = if typed {
            e.execute_typed(cmd, Origin::User)
        } else {
            e.execute(RoutePadWalkaround::NAME, serde_json::to_value(cmd).unwrap(), Origin::User)
        }
        .unwrap();
        assert_eq!(out.created.len(), 4);
        let after = e.read(Clone::clone);
        for (id, pair) in out.created.iter().zip(plan.plan.path.points.windows(2)) {
            let t = &after.board.tracks[id];
            assert_eq!((t.start, t.end, t.net.as_deref(), t.layer), (pair[0], pair[1], Some("N"), layer));
        }
        assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        e.redo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), after);
        let before = e.session_state();
        assert!(e.plan_pad_walkaround(&input(&e, ids)).is_ok());
        assert_eq!(e.session_state(), before);
    }
    // Foreign/unassigned layer copper avoids; same-net and opposite layer copper remain unchanged.
    for (net, layer, count) in [(Some("N"), CopperLayer::Top, 0), (Some("OTHER"), CopperLayer::Bottom, 0), (None, CopperLayer::Top, 1)] {
        let (mut project, ids) = fixture();
        barrier(&mut project, net, layer);
        let e = EngineHandle::new(project);
        let plan = e.plan_pad_walkaround(&input(&e, ids)).unwrap();
        assert_eq!(plan.plan.work.obstacle_boxes, count);
        assert_eq!(plan.plan.path.points.len(), if count == 0 { 2 } else { 5 });
    }
    // ALL via drill holes, even same-net or unassigned; foreign copper contributes a second box.
    for net in [Some("N"), Some("OTHER"), None] {
        let (mut project, ids) = fixture();
        let via = Via {
            id: ObjectId::new(),
            position: p(20_000_000, 10_000_000),
            diameter: Nm(1_000_001),
            drill: Nm(300_001),
            net: net.map(str::to_owned),
        };
        project.board.vias.insert(via.id, via);
        let e = EngineHandle::new(project);
        let plan = e.plan_pad_walkaround(&input(&e, ids)).unwrap();
        assert_eq!(plan.plan.work.obstacle_boxes, if net == Some("N") { 1 } else { 2 });
        assert!(plan.plan.path.points.len() > 2);
        assert!(plan.plan.path.points.iter().any(|p| p.y.0 == 9_000_000));
    }
    // Exact rotated foreign pad extents are swapped; bottom-side copper is irrelevant.
    for side in [Side::Top, Side::Bottom] {
        let (mut project, ids) = fixture();
        let mut obstacle = project.board.parts[&ids[0].part_id].clone();
        obstacle.id = ObjectId::new();
        obstacle.position = p(20_000_000, 10_000_000);
        obstacle.rotation_mdeg = 90_000;
        obstacle.side = side;
        let pads = &mut obstacle.pads.as_mut().unwrap().0;
        pads[0].id = ObjectId::new();
        pads[0].net = Some("OTHER".into());
        pads[0].width = Nm(4_000_001);
        pads[0].height = Nm(1_000_001);
        project.board.parts.insert(obstacle.id, obstacle);
        let e = EngineHandle::new(project);
        let plan = e.plan_pad_walkaround(&input(&e, ids)).unwrap();
        assert_eq!(plan.plan.work.obstacle_boxes, usize::from(side == Side::Top));
        assert_eq!(plan.plan.path.points.len(), if side == Side::Top { 5 } else { 2 });
    }
}
#[test]
fn failures_are_unchanged_with_complete_preflight_and_bounded_unrelated_content() {
    // Sequentially exercise refusal variants to avoid concurrent global worker admission in this test.
    let (mut project, ids) = fixture();
    barrier(&mut project, Some("FOREIGN"), CopperLayer::Top);
    let e = EngineHandle::new(project);
    let cmd = input(&e, ids);
    for bad in [
        RoutePadWalkaround { grid: Nm(0), ..cmd.clone() },
        RoutePadWalkaround { width: Nm(0), ..cmd.clone() },
        RoutePadWalkaround { target: ids[0], ..cmd.clone() },
        RoutePadWalkaround { width: Nm(20_000_000), ..cmd.clone() },
    ] {
        let before = e.session_state();
        assert!(e.plan_pad_walkaround(&bad).is_err());
        assert!(e.execute_typed(bad, Origin::User).is_err());
        assert_eq!(e.session_state(), before);
    }
    e.execute("update_part", serde_json::json!({"id":ids[1].part_id,"side":"bottom"}), Origin::agent("peer")).unwrap();
    let before = e.session_state();
    assert!(e.plan_pad_walkaround(&cmd).is_err());
    assert!(e.execute_typed(cmd, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    for variant in 0..10 {
        let (mut project, ids) = fixture();
        match variant {
            0 => {
                project.board.parts.get_mut(&ids[0].part_id).unwrap().pads = None;
            }
            1 => {
                project.board.parts.get_mut(&ids[0].part_id).unwrap().rotation_mdeg = 45_000;
            }
            2 => {
                project.board.outline =
                    Some(BoardOutline::new(Polygon::new(vec![p(0, 0), p(40_000_000, 0), p(20_000_000, 20_000_000)]), vec![]).unwrap());
            }
            3 => {
                project.board.outline.as_mut().unwrap().cutouts.push(Polygon::new(vec![
                    p(1_000_000, 1_000_000),
                    p(2_000_000, 1_000_000),
                    p(2_000_000, 2_000_000),
                    p(1_000_000, 2_000_000),
                ]));
            }
            4 => {
                let t = Track {
                    id: ObjectId::new(),
                    start: p(20_000_000, 0),
                    end: p(20_000_000, 20_000_000),
                    width: Nm(250_000),
                    layer: CopperLayer::Top,
                    net: Some("BLOCK".into()),
                };
                project.board.tracks.insert(t.id, t);
            }
            6 => {
                project.board.parts.get_mut(&ids[0].part_id).unwrap().rotation_mdeg = 360_000;
            }
            7 => {
                let part = project.board.parts.get_mut(&ids[0].part_id).unwrap();
                part.position.x = Nm(10_000_000_001);
                part.pads.as_mut().unwrap().0[0].position.x = Nm(-9_990_000_001);
            }
            8 => {
                let part = project.board.parts.get_mut(&ids[0].part_id).unwrap();
                part.position.x = Nm(-9_990_000_001);
                part.pads.as_mut().unwrap().0[0].position.x = Nm(10_000_000_001);
            }
            9 => {
                let part = project.board.parts.get_mut(&ids[0].part_id).unwrap();
                part.pads.as_mut().unwrap().0[0].number = "\n".into();
            }
            _ => {
                for i in 0..65 {
                    let t = Track {
                        id: ObjectId::new(),
                        start: p(20_000_000, i * 100_000),
                        end: p(21_000_000, i * 100_000),
                        width: Nm(250_000),
                        layer: CopperLayer::Top,
                        net: Some("BLOCK".into()),
                    };
                    project.board.tracks.insert(t.id, t);
                }
            }
        }
        let e = EngineHandle::new(project);
        let cmd = input(&e, ids);
        let before = e.session_state();
        assert!(e.plan_pad_walkaround(&cmd).is_err(), "variant {variant}");
        assert!(e.execute_typed(cmd, Origin::User).is_err(), "variant {variant}");
        assert_eq!(e.session_state(), before);
    }
    // A valid graph cannot promise success when complete joint DRC truncates due unrelated violations.
    let (mut project, ids) = fixture();
    for net in ["A", "B", "C"] {
        barrier(&mut project, Some(net), CopperLayer::Bottom);
    }
    let e = EngineHandle::new(project);
    let cmd = RoutePadWalkaround { rules: viamind_drc::DrcInput { max_violations: 1, ..Default::default() }, ..input(&e, ids) };
    let before = e.session_state();
    assert!(e.plan_pad_walkaround(&cmd).unwrap_err().to_string().contains("incomplete"));
    assert!(e.execute_typed(cmd, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    // Oversized unrelated schematic text is never cloned by immutable board-only planning.
    let (mut project, ids) = fixture();
    let mut sheet = viamind_doc::Sheet::new("Unrelated");
    sheet.name = "x".repeat(4_000_000);
    project.schematic.sheets.insert(sheet.id, sheet);
    let e = EngineHandle::new(project);
    let cmd = input(&e, ids);
    let before = e.event_cursor();
    assert!(e.plan_pad_walkaround(&cmd).is_ok());
    assert_eq!(e.event_cursor(), before);
    e.execute_typed(cmd, Origin::User).unwrap();
    assert_eq!(e.read(|p| p.schematic.sheets.values().next().unwrap().name.len()), 4_000_000);
}
