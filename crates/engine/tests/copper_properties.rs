use serde_json::json;
use viamind_doc::{CopperLayer, NamedNetMinima, ObjectId, Project, Track, Via};
use viamind_engine::{
    Command, CommandCall, CopperPropertyId, CopperPropertyObject, CopperPropertyQuery, EngineHandle, Origin, SetTrackProperties, SetViaProperties,
};
use viamind_geom::{Nm, Point};
fn p(x: i64, y: i64) -> Point {
    Point::new(Nm(x), Nm(y))
}
fn fixture() -> (Project, ObjectId, ObjectId) {
    let mut p = Project::new("stable copper properties");
    p.board.outline = Some(
        serde_json::from_value(json!({"outer":[{"x":0,"y":0},{"x":50000000,"y":0},{"x":50000000,"y":50000000},{"x":0,"y":50000000}],"cutouts":[]}))
            .unwrap(),
    );
    let t = Track {
        id: ObjectId::new(),
        start: pnt(5_000_003, 10_000_007),
        end: pnt(15_000_001, 10_000_007),
        width: Nm(250_000),
        layer: CopperLayer::Top,
        net: Some("N".into()),
    };
    let v = Via { id: ObjectId::new(), position: pnt(30_000_001, 25_000_003), diameter: Nm(1_000_000), drill: Nm(300_000), net: Some("N".into()) };
    let ids = (t.id, v.id);
    p.board.tracks.insert(t.id, t);
    p.board.vias.insert(v.id, v);
    (p, ids.0, ids.1)
}
fn pnt(x: i64, y: i64) -> Point {
    p(x, y)
}
fn zero() -> viamind_drc::DrcInput {
    viamind_drc::DrcInput {
        copper_clearance_mm: 0.,
        edge_clearance_mm: 0.,
        minimum_via_drill_mm: 0.,
        minimum_annular_width_mm: 0.,
        hole_clearance_mm: 0.,
        hole_to_copper_clearance_mm: 0.,
        max_violations: 1000,
    }
}
fn track(e: &EngineHandle, id: ObjectId) -> SetTrackProperties {
    SetTrackProperties {
        expected_project: e.read(|p| p.id),
        expected_cursor: e.event_cursor(),
        id,
        expected: e.read(|p| p.board.tracks[&id].clone()),
        width: Nm(350_001),
        layer: CopperLayer::Bottom,
        net: Some("N".into()),
        rules: zero(),
    }
}
fn via(e: &EngineHandle, id: ObjectId) -> SetViaProperties {
    SetViaProperties {
        expected_project: e.read(|p| p.id),
        expected_cursor: e.event_cursor(),
        id,
        expected: e.read(|p| p.board.vias[&id].clone()),
        diameter: Nm(900_003),
        drill: Nm(400_001),
        net: Some("N".into()),
        rules: zero(),
    }
}
fn reject<C: Command + Clone>(e: &EngineHandle, input: C, phrase: &str) {
    let before = e.session_state();
    let st = e.document_status();
    let error = e.execute_typed(input, Origin::User).unwrap_err().to_string();
    assert!(error.contains(phrase), "{error}");
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), st);
}
#[test]
fn track_and_via_keep_exact_geometry_ids_selection_and_one_undo_redo() {
    for dynamic in [false, true] {
        let (project, tid, vid) = fixture();
        let e = EngineHandle::new(project);
        e.set_selection(viamind_engine::SetSelection { ids: vec![tid, vid], mode: Default::default() }, Origin::User).unwrap();
        for is_via in [false, true] {
            let before = e.session_state();
            let out = if is_via {
                let i = via(&e, vid);
                if dynamic {
                    e.execute(SetViaProperties::NAME, serde_json::to_value(i).unwrap(), Origin::agent("peer"))
                } else {
                    e.execute_typed(i, Origin::User)
                }
            } else {
                let i = track(&e, tid);
                if dynamic {
                    e.execute(SetTrackProperties::NAME, serde_json::to_value(i).unwrap(), Origin::agent("peer"))
                } else {
                    e.execute_typed(i, Origin::User)
                }
            }
            .unwrap();
            assert!(out.created.is_empty());
            let after = e.session_state();
            assert_eq!(after.selection, before.selection);
            assert_eq!(after.cursor.sequence, before.cursor.sequence + 1);
            if is_via {
                let v = &after.project.board.vias[&vid];
                assert_eq!((v.id, v.position, v.diameter, v.drill), (vid, p(30_000_001, 25_000_003), Nm(900_003), Nm(400_001)));
                assert_eq!(after.project.board.tracks, before.project.board.tracks);
            } else {
                let t = &after.project.board.tracks[&tid];
                assert_eq!(
                    (t.id, t.start, t.end, t.width, t.layer),
                    (tid, p(5_000_003, 10_000_007), p(15_000_001, 10_000_007), Nm(350_001), CopperLayer::Bottom)
                );
                assert_eq!(after.project.board.vias, before.project.board.vias);
            }
            e.undo(Origin::User).unwrap();
            assert_eq!(e.read(Clone::clone), before.project);
            e.redo(Origin::User).unwrap();
            assert_eq!(e.read(Clone::clone), after.project);
            e.undo(Origin::User).unwrap();
        }
    }
}
#[test]
fn immutable_observation_has_exact_original_cursor_policy_without_selection_changes() {
    let (project, tid, vid) = fixture();
    let e = EngineHandle::new(project);
    let before = e.session_state();
    let status = e.document_status();
    for target in [CopperPropertyId::Track { id: tid }, CopperPropertyId::Via { id: vid }] {
        let s = e.query_copper_properties(&CopperPropertyQuery { target }).unwrap();
        assert_eq!(s.source_cursor, before.cursor);
        assert_eq!(s.project_id, before.project.id);
        assert!(s.document_net_rules.is_empty());
        match s.object {
            CopperPropertyObject::Track { track } => assert_eq!(track, before.project.board.tracks[&tid]),
            CopperPropertyObject::Via { via } => assert_eq!(via, before.project.board.vias[&vid]),
        };
    }
    assert!(e.query_copper_properties(&CopperPropertyQuery { target: CopperPropertyId::Track { id: vid } }).is_err());
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
}
#[test]
fn exact_document_width_and_drill_annulus_boundary_cannot_be_weakened_by_request() {
    let (mut project, tid, vid) = fixture();
    project.board.net_rules.insert(
        "N".into(),
        NamedNetMinima {
            minimum_copper_clearance: Nm(0),
            minimum_track_width: Nm(250_001),
            minimum_via_drill: Nm(300_001),
            minimum_annular_width: Nm(200_001),
        },
    );
    let e = EngineHandle::new(project);
    let mut t = track(&e, tid);
    t.width = Nm(250_000);
    reject(&e, t, "document track-width");
    let mut t = track(&e, tid);
    t.width = Nm(250_001);
    e.execute_typed(t, Origin::User).unwrap();
    assert_eq!(e.read(|p| p.board.tracks[&tid].width), Nm(250_001));
    let mut v = via(&e, vid);
    v.drill = Nm(300_000);
    reject(&e, v, "drill >= 300001");
    let mut v = via(&e, vid);
    v.drill = Nm(300_001);
    v.diameter = Nm(700_002);
    reject(&e, v, "annular width >= 200001");
    let mut v = via(&e, vid);
    v.drill = Nm(300_001);
    v.diameter = Nm(700_003);
    e.execute_typed(v, Origin::User).unwrap();
    assert_eq!(e.read(|p| p.board.vias[&vid].diameter), Nm(700_003));
    let mut v = via(&e, vid);
    v.rules.minimum_via_drill_mm = 0.400002;
    reject(&e, v, "drill >= 400002");
}
#[test]
fn foreign_collision_layer_change_and_same_net_drill_spacing_reject_atomically() {
    let (mut project, tid, vid) = fixture();
    let t = Track {
        id: ObjectId::new(),
        start: p(5_000_000, 10_600_007),
        end: p(15_000_000, 10_600_007),
        width: Nm(250_000),
        layer: CopperLayer::Top,
        net: Some("FOREIGN".into()),
    };
    project.board.tracks.insert(t.id, t.clone());
    let mut bottom = t;
    bottom.id = ObjectId::new();
    bottom.layer = CopperLayer::Bottom;
    bottom.start.y = Nm(10_000_007);
    bottom.end.y = Nm(10_000_007);
    project.board.tracks.insert(bottom.id, bottom);
    let mut nearby = project.board.vias[&vid].clone();
    nearby.id = ObjectId::new();
    nearby.position.x = Nm(30_700_001);
    project.board.vias.insert(nearby.id, nearby);
    let e = EngineHandle::new(project);
    let mut wide = track(&e, tid);
    wide.layer = CopperLayer::Top;
    wide.width = Nm(1_000_001);
    reject(&e, wide, "preflight failed");
    reject(&e, track(&e, tid), "preflight failed");
    let mut v = via(&e, vid);
    v.drill = Nm(1_200_001);
    v.diameter = Nm(2_000_001);
    reject(&e, v, "hole");
}
#[test]
fn edge_cutout_no_outline_and_exact_noop_refuse_without_events() {
    let (mut project, tid, vid) = fixture();
    project.board.vias.get_mut(&vid).unwrap().position.x = Nm(600_000);
    let e = EngineHandle::new(project);
    let mut v = via(&e, vid);
    v.diameter = Nm(2_000_001);
    reject(&e, v, "preflight failed");
    let mut t = track(&e, tid);
    t.width = t.expected.width;
    t.layer = t.expected.layer;
    t.net = t.expected.net.clone();
    reject(&e, t, "no property changes");
    let (mut project, tid, _) = fixture();
    project.board.outline = None;
    let e = EngineHandle::new(project);
    reject(&e, track(&e, tid), "explicit board outline");
    let (mut project, tid, _) = fixture();
    project.board.outline.as_mut().unwrap().cutouts = vec![viamind_geom::Polygon::new(vec![
        p(8_000_000, 10_300_000),
        p(12_000_000, 10_300_000),
        p(12_000_000, 14_000_000),
        p(8_000_000, 14_000_000),
    ])];
    let e = EngineHandle::new(project);
    let mut t = track(&e, tid);
    t.layer = CopperLayer::Top;
    t.width = Nm(1_000_000);
    reject(&e, t, "preflight failed");
}
#[test]
fn stale_sequences_streams_exact_expected_and_batch_reject_whole_transaction() {
    let (project, tid, vid) = fixture();
    let e = EngineHandle::new(project);
    let stale = track(&e, tid);
    e.execute_typed(via(&e, vid), Origin::agent("peer")).unwrap();
    reject(&e, stale, "session changed");
    let fresh = track(&e, tid);
    let mut wrong = via(&e, vid);
    wrong.expected.diameter = Nm(800_000);
    let before = e.session_state();
    let status = e.document_status();
    assert!(
        e.execute_batch(
            vec![
                CommandCall { name: SetTrackProperties::NAME.into(), input: serde_json::to_value(fresh).unwrap() },
                CommandCall { name: SetViaProperties::NAME.into(), input: serde_json::to_value(wrong).unwrap() }
            ],
            Origin::User,
            None
        )
        .is_err()
    );
    assert_eq!(e.session_state(), before);
    assert_eq!(e.document_status(), status);
    let mut stale = track(&e, tid);
    stale.expected_cursor.stream_id = EngineHandle::new(Project::new("other stream")).event_cursor().stream_id;
    reject(&e, stale, "session changed");
    let mut wrong = track(&e, tid);
    wrong.expected.width = Nm(1);
    reject(&e, wrong, "track changed");
}
#[test]
fn bounded_expected_and_replacement_nets_and_strict_identity_schema_refuse() {
    let (project, tid, _) = fixture();
    let e = EngineHandle::new(project);
    let input = serde_json::to_value(track(&e, tid)).unwrap();
    let before = e.session_state();
    for expected in [false, true] {
        let mut input = input.clone();
        if expected {
            input["expected"]["net"] = json!("x".repeat(257));
        } else {
            input["net"] = json!("x".repeat(257));
        }
        assert!(e.execute(SetTrackProperties::NAME, input, Origin::User).unwrap_err().to_string().contains("256"));
    }
    for changes in [json!({"kind":"track","id":tid,"width":1}), json!({"kind":"pad","id":tid})] {
        assert!(serde_json::from_value::<CopperPropertyId>(changes).is_err());
    }
    let mut extra = input;
    extra["position"] = json!({"x":0,"y":0});
    assert!(e.execute(SetTrackProperties::NAME, extra, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
}
#[test]
fn net_case_unassigned_and_document_floor_follow_replacement_without_inference() {
    let (mut project, tid, _) = fixture();
    project.board.net_rules.insert(
        "N".into(),
        NamedNetMinima { minimum_copper_clearance: Nm(0), minimum_track_width: Nm(500_001), minimum_via_drill: Nm(0), minimum_annular_width: Nm(0) },
    );
    let e = EngineHandle::new(project);
    let mut t = track(&e, tid);
    t.net = Some("n".into());
    e.execute_typed(t, Origin::User).unwrap();
    assert_eq!(e.read(|p| p.board.tracks[&tid].net.clone()), Some("n".into()));
    let mut t = track(&e, tid);
    t.net = None;
    e.execute_typed(t, Origin::User).unwrap();
    assert_eq!(e.read(|p| p.board.tracks[&tid].net.clone()), None);
    let mut t = track(&e, tid);
    t.net = Some("N".into());
    reject(&e, t, "document track-width");
}

#[test]
fn same_initial_cursor_batch_edits_both_objects_as_one_undo() {
    let (project, tid, vid) = fixture();
    let e = EngineHandle::new(project);
    let before = e.session_state();
    let t = track(&e, tid);
    let v = via(&e, vid);
    assert_eq!(t.expected_cursor, v.expected_cursor);
    let out = e
        .execute_batch(
            vec![
                CommandCall { name: SetTrackProperties::NAME.into(), input: serde_json::to_value(t).unwrap() },
                CommandCall { name: SetViaProperties::NAME.into(), input: serde_json::to_value(v).unwrap() },
            ],
            Origin::User,
            None,
        )
        .unwrap();
    assert_eq!(out.deltas.len(), 2);
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    assert_eq!(e.read(|p| p.board.tracks[&tid].width), Nm(350_001));
    assert_eq!(e.read(|p| p.board.vias[&vid].drill), Nm(400_001));
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
}
#[test]
fn unrelated_violations_may_remain_but_incomplete_reports_refuse_every_property() {
    for truncated in [false, true] {
        let (mut project, tid, _) = fixture();
        for (index, x) in [8_000_000, 12_000_000].into_iter().enumerate() {
            let t = Track {
                id: ObjectId::new(),
                start: if truncated { p(x, 9_000_000) } else { p(40_000_000, 40_000_000) },
                end: if truncated { p(x, 11_000_000) } else { p(45_000_000, 40_000_000) },
                width: Nm(250_000),
                layer: CopperLayer::Top,
                net: Some(format!("FOREIGN{index}")),
            };
            project.board.tracks.insert(t.id, t);
        }
        let e = EngineHandle::new(project);
        let mut t = track(&e, tid);
        t.layer = CopperLayer::Top;
        if truncated {
            t.rules.max_violations = 1;
            reject(&e, t, "preflight incomplete");
        } else {
            e.execute_typed(t, Origin::User).unwrap();
            let report = e.read(|p| viamind_drc::run_drc(p, &zero())).unwrap();
            assert!(!report.violations.is_empty());
            assert!(!report.truncated);
        }
    }
}
#[test]
fn real_candidate_text_growth_boundary_preserves_unused_large_metadata() {
    let (mut project, tid, _) = fixture();
    let huge = "x".repeat(3 * 1024 * 1024);
    let part = viamind_doc::Part {
        id: ObjectId::new(),
        assembly: Some(Box::new(viamind_doc::AssemblyMetadata { manufacturer: Some(huge.clone()), ..Default::default() })),
        schematic_symbol: None,
        designator: "L".into(),
        value: "x".repeat(2 * 1024 * 1024 - 4),
        footprint: String::new(),
        position: p(40_000_000, 5_000_000),
        rotation_mdeg: 0,
        side: viamind_doc::Side::Top,
        pads: None,
    };
    let part_id = part.id;
    project.board.parts.insert(part.id, part);
    project.name = huge.clone();
    let sheet = viamind_doc::Sheet::new(huge.clone());
    project.schematic.sheets.insert(sheet.id, sheet);
    let e = EngineHandle::new(project);
    let mut t = track(&e, tid);
    t.net = Some("NN".into());
    e.execute_typed(t, Origin::User).unwrap();
    assert!(e.read(|p| p.name == huge && p.board.parts[&part_id].assembly.as_ref().unwrap().manufacturer.as_ref() == Some(&huge)));
    let mut t = track(&e, tid);
    t.net = Some("NNN".into());
    reject(&e, t, "snapshot text");
}
#[test]
fn query_rejects_oversized_selected_text_and_policy_before_owned_copy() {
    for policy in [false, true] {
        let (mut project, tid, _) = fixture();
        if policy {
            project.board.net_rules.insert(
                "x".repeat(257),
                NamedNetMinima {
                    minimum_copper_clearance: Nm(0),
                    minimum_track_width: Nm(1),
                    minimum_via_drill: Nm(0),
                    minimum_annular_width: Nm(0),
                },
            );
        } else {
            project.board.tracks.get_mut(&tid).unwrap().net = Some("🙂".repeat(128));
        }
        let e = EngineHandle::new(project);
        let before = e.session_state();
        let status = e.document_status();
        assert!(e.query_copper_properties(&CopperPropertyQuery { target: CopperPropertyId::Track { id: tid } }).is_err());
        assert_eq!(e.session_state(), before);
        assert_eq!(e.document_status(), status);
    }
}

#[test]
fn advertised_nullable_net_is_required_exactly_like_command_deserialization() {
    let (project, tid, vid) = fixture();
    let e = EngineHandle::new(project);
    for (name, is_via) in [(SetTrackProperties::NAME, false), (SetViaProperties::NAME, true)] {
        let input = if is_via { serde_json::to_value(via(&e, vid)).unwrap() } else { serde_json::to_value(track(&e, tid)).unwrap() };
        let registry = viamind_engine::CommandRegistry::builtin();
        let schema = &registry.spec(name).unwrap().input_schema;
        assert!(schema["required"].as_array().unwrap().contains(&json!("net")));
        assert_eq!(schema["properties"]["net"]["type"], json!(["string", "null"]));
        assert!(schema["properties"]["expected"]["required"].as_array().unwrap().contains(&json!("net")));
        assert_eq!(schema["properties"]["expected"]["properties"]["net"]["type"], json!(["string", "null"]));
        for original in [false, true] {
            let mut missing = input.clone();
            if original {
                missing["expected"].as_object_mut().unwrap().remove("net");
            } else {
                missing.as_object_mut().unwrap().remove("net");
            }
            let before = e.session_state();
            assert!(e.execute(name, missing, Origin::User).is_err());
            assert_eq!(e.session_state(), before);
        }
        let mut explicit = input;
        explicit["net"] = serde_json::Value::Null;
        let before = e.read(Clone::clone);
        e.execute(name, explicit, Origin::User).unwrap();
        e.undo(Origin::User).unwrap();
        assert_eq!(e.read(Clone::clone), before);
    }
}
