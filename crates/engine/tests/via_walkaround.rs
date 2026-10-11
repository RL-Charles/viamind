use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Pad, Pads, Part, Project, Side, Track, Via};
use viamind_engine::{EngineHandle, Origin, PadEndpointId, RoutePadViaWalkaround};
use viamind_geom::{Nm, Point, Polygon};
static TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn rectangle(x: i64, y: i64, xx: i64, yy: i64) -> Polygon {
    Polygon::new(vec![p(x, y), p(xx, y), p(xx, yy), p(x, yy)])
}
fn fixture() -> (Project, [PadEndpointId; 2]) {
    let mut project = Project::new("Two-side checked walkaround");
    project.board.outline = Some(BoardOutline::new(rectangle(0, 0, 60_000_000, 30_000_000), vec![]).unwrap());
    let ids = [(10_000_000, Side::Top), (50_000_000, Side::Bottom)].map(|(x, side)| {
        let pad = Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: p(123, 129),
            width: Nm(1_000_001),
            height: Nm(1_000_001),
            net: Some("N".into()),
        };
        let part = Part {
            id: ObjectId::new(),
            schematic_symbol: None,
            assembly: None,
            designator: format!("J{x}"),
            value: String::new(),
            footprint: String::new(),
            position: p(x, 10_000_000),
            rotation_mdeg: 0,
            side,
            pads: Some(Pads::try_from(vec![pad.clone()]).unwrap()),
        };
        let id = PadEndpointId { part_id: part.id, pad_id: pad.id };
        project.board.parts.insert(part.id, part);
        id
    });
    for (x, layer) in [(20_000_000, CopperLayer::Top), (40_000_000, CopperLayer::Bottom)] {
        let track =
            Track { id: ObjectId::new(), start: p(x, 8_000_000), end: p(x, 12_000_000), width: Nm(250_000), layer, net: Some("FOREIGN".into()) };
        project.board.tracks.insert(track.id, track);
    }
    (project, ids)
}
fn input(e: &EngineHandle, ids: [PadEndpointId; 2]) -> RoutePadViaWalkaround {
    RoutePadViaWalkaround {
        expected_project: e.read(|p| p.id),
        expected_cursor: e.event_cursor(),
        start: ids[0],
        target: ids[1],
        via: p(30_000_000, 10_000_000),
        grid: Nm(1_000_000),
        width: Nm(250_001),
        via_diameter: Nm(600_000),
        via_drill: Nm(300_000),
        rules: Default::default(),
    }
}
fn reject(e: &EngineHandle, cmd: RoutePadViaWalkaround) {
    let before = e.session_state();
    assert!(e.plan_pad_via_walkaround(&cmd).is_err());
    assert!(e.execute_typed(cmd, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
}
#[test]
fn both_side_paths_ignore_unrelated_invalid_sourcing_without_mutation() {
    let _guard = TEST.lock().unwrap();
    let (mut project, ids) = fixture();
    let plain = EngineHandle::new(project.clone());
    let expected = plain.plan_pad_via_walkaround(&input(&plain, ids)).unwrap();
    for part in project.board.parts.values_mut() {
        part.assembly =
            Some(Box::new(viamind_doc::AssemblyMetadata { mpn: Some("\n".repeat(3 * 1024 * 1024)), populate: false, ..Default::default() }));
    }
    let e = EngineHandle::new(project);
    let cursor = e.event_cursor();
    let plan = e.plan_pad_via_walkaround(&input(&e, ids)).unwrap();
    assert_eq!(plan.before.path.points, expected.before.path.points);
    assert_eq!(plan.after.path.points, expected.after.path.points);
    assert_eq!(plan.work, expected.work);
    assert_eq!(e.event_cursor(), cursor);
    assert!(e.read(|p| p.board.parts.values().all(|part| {
        let metadata = part.assembly.as_ref().unwrap();
        !metadata.populate && metadata.mpn.as_ref().unwrap().len() == 3 * 1024 * 1024
    })));
}
#[test]
fn independently_expected_layer_paths_joint_commit_and_one_undo() {
    let _guard = TEST.lock().unwrap();
    let (mut project, ids) = fixture();
    project.board.outline.as_mut().unwrap().cutouts.push(rectangle(34_000_000, 20_000_000, 36_000_000, 22_000_000));
    let e = EngineHandle::new(project);
    let cmd = input(&e, ids);
    let before = e.session_state();
    let plan = e.plan_pad_via_walkaround(&cmd).unwrap();
    let first = vec![
        p(10_000_123, 10_000_129),
        p(10_000_000, 10_000_000),
        p(10_000_000, 7_000_000),
        p(21_000_000, 7_000_000),
        p(21_000_000, 10_000_000),
        p(30_000_000, 10_000_000),
    ];
    let second = vec![
        p(30_000_000, 10_000_000),
        p(39_000_000, 10_000_000),
        p(39_000_000, 7_000_000),
        p(41_000_000, 7_000_000),
        p(41_000_000, 10_000_000),
        p(50_000_000, 10_000_000),
        p(49_999_877, 10_000_129),
    ];
    assert_eq!(plan.before.path.points, first);
    assert_eq!(plan.after.path.points, second);
    assert_eq!(plan.work.obstacle_boxes, 4);
    assert_eq!(plan.work.graph_nodes, plan.before.work.graph_nodes + plan.after.work.graph_nodes);
    assert_eq!(plan.work.intersection_tests, plan.before.work.intersection_tests + plan.after.work.intersection_tests);
    assert_eq!((plan.start_layer, plan.target_layer, plan.net.as_str()), (CopperLayer::Top, CopperLayer::Bottom, "N"));
    assert!(plan.complete_scoped_preflight);
    assert_eq!(e.session_state(), before);
    let out = e.execute("route_pad_via_walkaround", serde_json::to_value(cmd.clone()).unwrap(), Origin::agent("joint")).unwrap();
    let after = e.session_state();
    assert_eq!(out.created.len(), 12);
    assert_eq!(after.cursor.sequence, before.cursor.sequence + 1);
    for (index, pair) in first.windows(2).chain(second.windows(2)).enumerate() {
        let t = &after.project.board.tracks[&out.created[index]];
        assert_eq!((t.start, t.end, t.layer), (pair[0], pair[1], if index < 5 { CopperLayer::Top } else { CopperLayer::Bottom }));
    }
    let v = &after.project.board.vias[&out.created[11]];
    assert_eq!((v.position, v.diameter, v.drill, v.net.as_deref()), (cmd.via, cmd.via_diameter, cmd.via_drill, Some("N")));
    reject(&e, cmd);
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), after.project);
}
#[test]
fn individual_paths_do_not_publish_when_joint_via_or_input_fails() {
    let _guard = TEST.lock().unwrap();
    let (project, ids) = fixture();
    let e = EngineHandle::new(project);
    let base = input(&e, ids);
    // Track centerline fits at x1mm, but this via radius + edge clearance does not.
    reject(&e, RoutePadViaWalkaround { via: p(1_000_000, 10_000_000), via_diameter: Nm(3_000_000), ..base.clone() });
    reject(&e, RoutePadViaWalkaround { via_diameter: Nm(400_000), ..base.clone() }); // radial annulus below0.1mm
    reject(&e, RoutePadViaWalkaround { via: p(30_000_001, 10_000_000), ..base.clone() });
    reject(&e, RoutePadViaWalkaround { grid: Nm(0), ..base.clone() });
    reject(&e, RoutePadViaWalkaround { target: ids[0], ..base.clone() });
    // Existing same-net hole stays outside the track clearance boxes (0.475001mm)
    // while the two drill edges require0.55mm center spacing. Joint DRC must refuse.
    let (mut project, ids) = fixture();
    let via = Via { id: ObjectId::new(), position: p(30_000_000, 10_500_000), diameter: Nm(600_000), drill: Nm(300_000), net: Some("N".into()) };
    project.board.vias.insert(via.id, via);
    let e = EngineHandle::new(project);
    let cmd = input(&e, ids);
    let error = e.plan_pad_via_walkaround(&cmd).unwrap_err().to_string();
    assert!(error.contains("hole"), "{error}");
    reject(&e, cmd);
}
#[test]
fn both_sides_charge_each_cutout_against_one_aggregate_cap() {
    let _guard = TEST.lock().unwrap();
    for count in [32, 33] {
        let (mut project, ids) = fixture();
        project.board.tracks.clear();
        project.board.outline.as_mut().unwrap().cutouts =
            (0..count).map(|i| rectangle(1_000_000 + i * 1_000_000, 20_000_000, 1_100_000 + i * 1_000_000, 20_100_000)).collect();
        let e = EngineHandle::new(project);
        let cmd = input(&e, ids);
        if count == 32 {
            let checked = e.plan_pad_via_walkaround(&cmd).unwrap();
            assert_eq!(checked.work.obstacle_boxes, 64);
        } else {
            assert!(e.plan_pad_via_walkaround(&cmd).unwrap_err().to_string().contains("limit64"));
            reject(&e, cmd);
        }
    }
}

#[test]
fn real_peer_edit_and_reopened_stream_reject_typed_dynamic_and_grouped_requests() {
    use viamind_engine::{CommandCall, OpenProject, SaveProject};
    let _guard = TEST.lock().unwrap();
    let (project, ids) = fixture();
    let e = EngineHandle::new(project);
    let stale = input(&e, ids);
    e.execute(
        "add_track",
        serde_json::json!({"start_x_mm":55,"start_y_mm":25,"end_x_mm":56,"end_y_mm":25,"width_mm":0.25,"layer":"top","net":"PEER"}),
        Origin::agent("peer"),
    )
    .unwrap();
    reject(&e, stale.clone());
    let before = e.session_state();
    assert!(e.execute("route_pad_via_walkaround", serde_json::to_value(stale.clone()).unwrap(), Origin::User).is_err());
    assert!(
        e.execute_batch(
            vec![
                CommandCall { name: "add_via".into(), input: serde_json::json!({"x_mm":58,"y_mm":25,"diameter_mm":0.6,"drill_mm":0.3,"net":"PEER"}) },
                CommandCall { name: "route_pad_via_walkaround".into(), input: serde_json::to_value(stale).unwrap() }
            ],
            Origin::User,
            None
        )
        .is_err()
    );
    assert_eq!(e.session_state(), before);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same-project.viamind.json");
    e.save_project(SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    let stale = input(&e, ids);
    e.open_project(OpenProject { path, discard_dirty: false, expected_cursor: Some(stale.expected_cursor) }, Origin::User).unwrap();
    assert_ne!(e.event_cursor().stream_id, stale.expected_cursor.stream_id);
    reject(&e, stale);
}
