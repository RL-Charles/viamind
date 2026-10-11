use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Pad, Pads, Part, Project, Side, Track, Via};
use viamind_engine::{CleanupCardinalTracks, CleanupTracks, Command, CommandCall, EngineHandle, Origin};
use viamind_geom::{Nm, Point, Polygon};

fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn fixture() -> Project {
    let mut project = Project::new("Equivalent cardinal cleanup");
    project.board.outline = Some(
        BoardOutline::new(
            Polygon::new(vec![p(-50_000_000, -50_000_000), p(50_000_000, -50_000_000), p(50_000_000, 50_000_000), p(-50_000_000, 50_000_000)]),
            vec![],
        )
        .unwrap(),
    );
    project
}
fn track(project: &mut Project, a: Point, b: Point, layer: CopperLayer) -> ObjectId {
    let t = Track { id: ObjectId::new(), start: a, end: b, width: Nm(250_001), layer, net: Some("N".into()) };
    let id = t.id;
    project.board.tracks.insert(id, t);
    id
}
fn request(e: &EngineHandle) -> CleanupCardinalTracks {
    CleanupCardinalTracks {
        expected_project: e.read(|p| p.id),
        expected_cursor: e.event_cursor(),
        tracks: CleanupTracks::Net { net: "N".into(), layer: None },
        rules: Default::default(),
    }
}
fn reject(e: &EngineHandle, input: CleanupCardinalTracks) {
    let before = e.session_state();
    let status = e.document_status();
    assert!(e.plan_cardinal_track_cleanup(&input).is_err());
    assert!(e.execute_typed(input, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
}
#[test]
fn exact_signed_chains_survivor_ids_full_session_immutability_and_atomic_undo() {
    for layer in [CopperLayer::Top, CopperLayer::Bottom] {
        let mut project = fixture();
        let ids = [
            track(&mut project, p(10_000_003, -10_000_007), p(3, -10_000_007), layer),
            track(&mut project, p(-10_000_001, -10_000_007), p(3, -10_000_007), layer),
            track(&mut project, p(10_000_003, -10_000_007), p(20_000_005, -10_000_007), layer),
        ];
        let vertical = [
            track(&mut project, p(-20_000_003, 5_000_001), p(-20_000_003, 15_000_003), layer),
            track(&mut project, p(-20_000_003, -5_000_001), p(-20_000_003, 5_000_001), layer),
        ];
        let e = EngineHandle::new(project);
        let input = request(&e);
        let before = e.session_state();
        let plan = e.plan_cardinal_track_cleanup(&input).unwrap();
        assert_eq!(e.session_state(), before);
        assert_eq!((plan.counts.selected_tracks, plan.counts.merged_chains, plan.counts.deleted_tracks), (5, 2, 3));
        for (chain, a, b) in [
            (ids.as_slice(), p(-10_000_001, -10_000_007), p(20_000_005, -10_000_007)),
            (vertical.as_slice(), p(-20_000_003, -5_000_001), p(-20_000_003, 15_000_003)),
        ] {
            let expected = *chain.iter().min().unwrap();
            let replacement = plan.replacements.iter().find(|r| r.after.id == expected).unwrap();
            assert_eq!(
                (replacement.after.start, replacement.after.end, replacement.after.width, replacement.after.layer),
                (a, b, Nm(250_001), layer)
            );
        }
        e.execute_typed(input, Origin::User).unwrap();
        assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
        let after = e.read(Clone::clone);
        assert_eq!(after.board.tracks.len(), 2);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        e.redo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), after);
    }
}
#[test]
fn branches_width_net_diagonal_and_unselected_neighbors_are_retained_without_events() {
    for condition in 0..6 {
        let mut project = fixture();
        let a = track(&mut project, p(-10_000_000, 0), p(0, 0), CopperLayer::Top);
        let b = track(&mut project, p(0, 0), p(10_000_000, 0), CopperLayer::Top);
        match condition {
            0 => {
                track(&mut project, p(0, 0), p(0, 10_000_000), CopperLayer::Top);
            }
            1 => project.board.tracks.get_mut(&b).unwrap().width = Nm(250_003),
            2 => project.board.tracks.get_mut(&b).unwrap().net = Some("OTHER".into()),
            3 => project.board.tracks.get_mut(&b).unwrap().end.y = Nm(10_000_000),
            4 => project.board.tracks.get_mut(&b).unwrap().net = None,
            _ => {}
        }
        let e = EngineHandle::new(project);
        let mut input = request(&e);
        input.tracks = CleanupTracks::Ids { ids: if condition == 5 { vec![a] } else { vec![a, b] } };
        let before = e.session_state();
        let plan = e.plan_cardinal_track_cleanup(&input).unwrap();
        assert_eq!(plan.counts.deleted_tracks, 0);
        if condition == 3 {
            assert_eq!(plan.counts.unsupported_diagonal_tracks, 1);
        }
        e.execute_typed(input, Origin::User).unwrap();
        assert_eq!(e.session_state(), before, "no-op must not emit an event");
    }
}
fn pad(project: &mut Project, center: Point, side: Side, rotation: i32) {
    let local = p(123, 129);
    let mut part = Part {
        id: ObjectId::new(),
        schematic_symbol: None,
        designator: format!("J{}", project.board.parts.len()),
        value: String::new(),
        footprint: String::new(),
        position: p(0, 0),
        rotation_mdeg: rotation,
        side,
        pads: Some(
            Pads::try_from(vec![Pad {
                id: ObjectId::new(),
                number: "1".into(),
                position: local,
                width: Nm(500_001),
                height: Nm(500_001),
                net: Some("N".into()),
            }])
            .unwrap(),
        ),
    };
    let delta = part.local_to_world_nm_exact(local).unwrap();
    part.position = p(center.x.0 - delta.x.0, center.y.0 - delta.y.0);
    project.board.parts.insert(part.id, part);
}
#[test]
fn exact_mirrored_pad_anchors_and_all_net_hole_contact_preserve_guides() {
    for side in [Side::Top, Side::Bottom] {
        let layer = if side == Side::Bottom { CopperLayer::Bottom } else { CopperLayer::Top };
        let mut project = fixture();
        track(&mut project, p(-10_000_000, 0), p(0, 0), layer);
        track(&mut project, p(0, 0), p(10_000_000, 0), layer);
        for center in [p(-10_000_000, 0), p(0, 0), p(10_000_000, 0)] {
            pad(&mut project, center, side, 270_000);
        }
        let e = EngineHandle::new(project);
        let before = e.read(viamind_engine::connection_guides);
        let plan = e.plan_cardinal_track_cleanup(&request(&e)).unwrap();
        assert_eq!(plan.counts.deleted_tracks, 0);
        assert!(plan.counts.retained_anchor_joints > 0);
        e.execute_typed(request(&e), Origin::User).unwrap();
        assert_eq!(serde_json::to_value(e.read(viamind_engine::connection_guides)).unwrap(), serde_json::to_value(before).unwrap());
    }
    for net in [None, Some("N"), Some("OTHER")] {
        let mut project = fixture();
        track(&mut project, p(-10_000_000, 0), p(0, 0), CopperLayer::Top);
        track(&mut project, p(0, 0), p(10_000_000, 0), CopperLayer::Top);
        let via =
            Via { id: ObjectId::new(), position: p(4_000_000, 150_001), diameter: Nm(600_003), drill: Nm(300_001), net: net.map(str::to_owned) };
        project.board.vias.insert(via.id, via);
        let e = EngineHandle::new(project);
        let plan = e.plan_cardinal_track_cleanup(&request(&e)).unwrap();
        assert_eq!(
            (plan.counts.retained_hole_chains, plan.counts.deleted_tracks, plan.counts.hole_candidates),
            (1, 0, 1),
            "closed odd-Nm hole AABB contact must retain chain"
        );
    }
}
#[test]
fn successful_cleanup_preserves_existing_guide_endpoints_and_duplicate_full_track_retains_chain() {
    let mut project = fixture();
    track(&mut project, p(-10_000_000, 0), p(0, 0), CopperLayer::Top);
    track(&mut project, p(0, 0), p(10_000_000, 0), CopperLayer::Top);
    for center in [p(-10_000_000, 0), p(10_000_000, 0), p(20_000_000, 10_000_000)] {
        pad(&mut project, center, Side::Top, 90_000);
    }
    let duplicate_project = {
        let mut duplicate = project.clone();
        track(&mut duplicate, p(-10_000_000, 0), p(10_000_000, 0), CopperLayer::Top);
        duplicate
    };
    let e = EngineHandle::new(project);
    let before = serde_json::to_value(e.read(viamind_engine::connection_guides)).unwrap();
    assert_eq!(e.plan_cardinal_track_cleanup(&request(&e)).unwrap().counts.deleted_tracks, 1);
    e.execute_typed(request(&e), Origin::User).unwrap();
    assert_eq!(serde_json::to_value(e.read(viamind_engine::connection_guides)).unwrap(), before);
    let e = EngineHandle::new(duplicate_project);
    let plan = e.plan_cardinal_track_cleanup(&request(&e)).unwrap();
    assert_eq!((plan.counts.retained_duplicate_chains, plan.counts.deleted_tracks), (1, 0));
}
#[test]
fn guarded_queries_typed_dynamic_batches_and_same_uuid_reopen_are_atomic() {
    let mut project = fixture();
    let ids = [track(&mut project, p(-10_000_000, 0), p(0, 0), CopperLayer::Top), track(&mut project, p(0, 0), p(10_000_000, 0), CopperLayer::Top)];
    let e = EngineHandle::new(project);
    let stale = request(&e);
    e.execute("add_part", serde_json::json!({"designator":"R1","value":"1k","footprint":"R","x_mm":30,"y_mm":30}), Origin::agent("peer")).unwrap();
    reject(&e, stale.clone());
    let before = e.session_state();
    assert!(e.execute(CleanupCardinalTracks::NAME, serde_json::to_value(stale.clone()).unwrap(), Origin::User).is_err());
    let good = request(&e);
    assert!(
        e.execute_batch(
            vec![
                CommandCall { name: CleanupCardinalTracks::NAME.into(), input: serde_json::to_value(&good).unwrap() },
                CommandCall { name: CleanupCardinalTracks::NAME.into(), input: serde_json::to_value(stale).unwrap() }
            ],
            Origin::User,
            None
        )
        .is_err()
    );
    assert_eq!(e.session_state(), before);
    for tracks in [
        CleanupTracks::Ids { ids: vec![ids[0], ids[0]] },
        CleanupTracks::Ids { ids: vec![ObjectId::new()] },
        CleanupTracks::Ids { ids: vec![ids[0]; 2049] },
    ] {
        reject(&e, CleanupCardinalTracks { tracks, ..good.clone() });
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cleanup.viamind.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), ..Default::default() }).unwrap();
    let stale = request(&e);
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::User).unwrap();
    reject(&e, stale);
}
#[test]
fn affected_joint_drc_failure_unsupported_transforms_and_merge_budget_reject_whole_operation() {
    let mut project = fixture();
    track(&mut project, p(-10_000_000, 0), p(0, 0), CopperLayer::Top);
    track(&mut project, p(0, 0), p(10_000_000, 0), CopperLayer::Top);
    let foreign = track(&mut project, p(-10_000_000, 300_000), p(10_000_000, 300_000), CopperLayer::Top);
    project.board.tracks.get_mut(&foreign).unwrap().net = Some("FOREIGN".into());
    let e = EngineHandle::new(project);
    reject(&e, request(&e));
    let mut project = fixture();
    for i in 0..1026 {
        track(&mut project, p(i * 10_000 + 1_000_000, 0), p((i + 1) * 10_000 + 1_000_000, 0), CopperLayer::Top);
    }
    let e = EngineHandle::new(project);
    assert!(e.plan_cardinal_track_cleanup(&request(&e)).unwrap_err().to_string().contains("deletion limit"));
    reject(&e, request(&e));
    let mut project = fixture();
    pad(&mut project, p(0, 0), Side::Top, 0);
    project.board.parts.values_mut().next().unwrap().rotation_mdeg = 45_000;
    let e = EngineHandle::new(project);
    reject(&e, request(&e));
}

#[test]
fn unrelated_schematic_is_preserved_capture_text_and_net_match_limits_precede_copy() {
    let mut project = fixture();
    track(&mut project, p(-10_000_000, 0), p(0, 0), CopperLayer::Top);
    track(&mut project, p(0, 0), p(10_000_000, 0), CopperLayer::Top);
    let sheet = viamind_doc::Sheet::new("unused".repeat(600_000));
    project.schematic.sheets.insert(sheet.id, sheet);
    let e = EngineHandle::new(project);
    let before = e.read(|p| p.schematic.clone());
    assert_eq!(e.plan_cardinal_track_cleanup(&request(&e)).unwrap().counts.deleted_tracks, 1);
    e.execute_typed(request(&e), Origin::User).unwrap();
    assert_eq!(e.read(|p| p.schematic.clone()), before);
    let mut project = fixture();
    pad(&mut project, p(0, 0), Side::Top, 0);
    project.board.parts.values_mut().next().unwrap().value = "x".repeat(2 * 1024 * 1024 + 1);
    let e = EngineHandle::new(project);
    assert!(e.plan_cardinal_track_cleanup(&request(&e)).unwrap_err().to_string().contains("snapshot text"));
    reject(&e, request(&e));
    let mut project = fixture();
    for i in 0..2049 {
        track(&mut project, p(i * 1000, 0), p(i * 1000 + 1, 0), CopperLayer::Top);
    }
    let e = EngineHandle::new(project);
    assert!(e.plan_cardinal_track_cleanup(&request(&e)).unwrap_err().to_string().contains("selector exceeds"));
    reject(&e, request(&e));
}

#[test]
fn incomplete_joint_report_refuses_even_with_only_unrelated_existing_violations() {
    let mut project = fixture();
    track(&mut project, p(-10_000_000, 0), p(0, 0), CopperLayer::Top);
    track(&mut project, p(0, 0), p(10_000_000, 0), CopperLayer::Top);
    for (a, b, y, net) in
        [(20_000_000, 30_000_000, 20_000_000, "A"), (20_000_000, 30_000_000, 20_100_000, "B"), (20_000_000, 30_000_000, 20_200_000, "C")]
    {
        let id = track(&mut project, p(a, y), p(b, y), CopperLayer::Top);
        project.board.tracks.get_mut(&id).unwrap().net = Some(net.into());
    }
    let e = EngineHandle::new(project);
    let mut input = request(&e);
    input.rules.max_violations = 1;
    assert!(e.plan_cardinal_track_cleanup(&input).unwrap_err().to_string().contains("incomplete"));
    reject(&e, input);
}
