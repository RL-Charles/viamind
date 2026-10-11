use serde_json::json;
use viamind_doc::{BoardOutline, Project};
use viamind_engine::EngineHandle;
use viamind_mcp::{CallContext, engine_registry};
#[tokio::test]
async fn checked_cardinal_cleanup_is_read_only_guarded_and_one_undo_for_two_agents() {
    use viamind_doc::{CopperLayer, ObjectId, Track};
    let mut project = Project::new("MCP cleanup");
    project.board.outline = Some(
        serde_json::from_value(json!({"outer":[{"x":0,"y":0},{"x":50000000,"y":0},{"x":50000000,"y":30000000},{"x":0,"y":30000000}],"cutouts":[]}))
            .unwrap(),
    );
    for (a, b) in [(10_000_123, 20_000_127), (20_000_127, 30_000_129)] {
        let track: Track = serde_json::from_value(json!({"id":ObjectId::new(),"start":{"x":a,"y":10_000_007},"end":{"x":b,"y":10_000_007},"width":250_001,"layer":CopperLayer::Top,"net":"N"})).unwrap();
        project.board.tracks.insert(track.id, track);
    }
    let e = EngineHandle::new(project);
    let registry = engine_registry(e.clone()).unwrap();
    let a = CallContext::new("cleanup-a");
    let b = CallContext::new("cleanup-b");
    let before = e.session_state();
    let input = json!({"expected_project":before.project.id,"expected_cursor":before.cursor,"tracks":{"source":"net","net":"N","layer":"top"}});
    let plan = registry.call("plan_cardinal_track_cleanup", input.clone(), b.clone()).await.unwrap();
    assert_eq!(plan["counts"]["deleted_tracks"], 1);
    assert_eq!(plan["replacements"][0]["after"]["start"]["x"], 10_000_123);
    assert_eq!(plan["replacements"][0]["after"]["end"]["x"], 30_000_129);
    assert_eq!(e.session_state(), before);
    registry.call("cleanup_cardinal_tracks", input.clone(), a.clone()).await.unwrap();
    let after = e.read(Clone::clone);
    assert_eq!(after.board.tracks.len(), 1);
    assert!(registry.call("cleanup_cardinal_tracks", input.clone(), b.clone()).await.is_err());
    assert!(registry.call("plan_cardinal_track_cleanup", input, b).await.is_err());
    assert_eq!(e.read(Clone::clone), after);
    registry.call("undo", json!({}), a.clone()).await.unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    registry.call("redo", json!({}), a).await.unwrap();
    assert_eq!(e.read(Clone::clone), after);
}
#[tokio::test]
async fn independent_agents_route_atomic_polylines_and_reject_stale_session_cursors() {
    let mut project = Project::new("MCP route");
    let p = |x, y| json!({"x":x,"y":y});
    project.board.outline = Some(
        serde_json::from_value::<BoardOutline>(json!({"outer":[p(0,0),p(100_000_000,0),p(100_000_000,100_000_000),p(0,100_000_000)],"cutouts":[]}))
            .unwrap(),
    );
    let e = EngineHandle::new(project);
    let registry = engine_registry(e.clone()).unwrap();
    let a = CallContext::new("route-a");
    let b = CallContext::new("route-b");
    let state = registry.call("get_session_state", json!({}), a.clone()).await.unwrap();
    let input = json!({"expected_project":state["project"]["id"],"expected_cursor":state["cursor"],"points":[p(10_000_000,10_000_000),p(30_000_000,10_000_000),p(30_000_000,20_000_000)],"grid":250000,"width":250000,"layer":"top","net":"GND"});
    let result = registry.call("route_polyline", input.clone(), a.clone()).await.unwrap();
    assert_eq!(result["created"].as_array().unwrap().len(), 2);
    let before = e.session_state();
    assert!(registry.call("route_polyline", input, b).await.unwrap_err().to_string().contains("session changed"));
    assert_eq!(e.session_state(), before);
    registry.call("undo", json!({}), a.clone()).await.unwrap();
    assert!(e.read(|p| p.board.tracks.is_empty()));
    registry.call("redo", json!({}), a).await.unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
}
#[tokio::test]
async fn immutable_pad_discovery_resolves_exact_identity_and_derives_net_side_with_agent_guards() {
    use viamind_doc::{ObjectId, Pad, Pads, Part, Side};
    let mut project = Project::new("MCP explicit endpoints");
    project.board.outline = Some(
        serde_json::from_value(
            json!({"outer":[{"x":0,"y":0},{"x":100000000,"y":0},{"x":100000000,"y":100000000},{"x":0,"y":100000000}],"cutouts":[]}),
        )
        .unwrap(),
    );
    for (i, x) in [10_000_123, 30_000_257].into_iter().enumerate() {
        let pad = Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: serde_json::from_value(json!({"x":0,"y":0})).unwrap(),
            width: serde_json::from_value(json!(1000000)).unwrap(),
            height: serde_json::from_value(json!(1000000)).unwrap(),
            net: Some("EXACT".into()),
            apertures: None,
        };
        let part = Part {
            holes: None,
            artwork: None,
            assembly: None,
            schematic_symbol: None,
            schematic_package: None,
            id: ObjectId::new(),
            designator: format!("J{i}"),
            value: String::new(),
            footprint: String::new(),
            position: serde_json::from_value(json!({"x":x,"y":10000129})).unwrap(),
            rotation_mdeg: 0,
            side: Side::Bottom,
            pads: Some(Pads::try_from(vec![pad]).unwrap()),
        };
        project.board.parts.insert(part.id, part);
    }
    let e = EngineHandle::new(project);
    let registry = engine_registry(e.clone()).unwrap();
    let a = CallContext::new("endpoint-a");
    let b = CallContext::new("endpoint-b");
    let before = e.session_state();
    let page = registry.call("query_route_endpoints", json!({"net":"EXACT"}), a.clone()).await.unwrap();
    assert_eq!(page["total"], 2);
    assert_eq!(page["endpoints"][0]["layer"], "bottom");
    assert_eq!(e.session_state(), before);
    let input = json!({"expected_project":page["project_id"],"expected_cursor":page["source_cursor"],"start":page["endpoints"][0]["id"],"target":page["endpoints"][1]["id"],"interior":[],"grid":250000,"width":250000});
    let result = registry.call("route_pad_polyline", input.clone(), a.clone()).await.unwrap();
    assert_eq!(result["created"].as_array().unwrap().len(), 1);
    let after = e.session_state();
    let track = e.read(|p| p.board.tracks.values().next().unwrap().clone());
    assert_eq!(track.layer, viamind_doc::CopperLayer::Bottom);
    assert_eq!(track.net.as_deref(), Some("EXACT"));
    assert_eq!(serde_json::to_value(track.start.y).unwrap(), json!(10000129));
    assert_ne!(track.start.x.0 % 250000, 0);
    assert!(registry.call("route_pad_polyline", input, b).await.unwrap_err().to_string().contains("session changed"));
    assert_eq!(e.session_state(), after);
    registry.call("undo", json!({}), a.clone()).await.unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    registry.call("redo", json!({}), a).await.unwrap();
    assert_eq!(e.read(Clone::clone), after.project);
}
#[tokio::test]

async fn opposite_side_via_route_is_discoverable_atomic_and_guarded_for_two_agents() {
    use viamind_doc::{ObjectId, Pad, Pads, Part, Side};
    let mut project = Project::new("MCP explicit endpoints");
    project.board.outline = Some(
        serde_json::from_value(
            json!({"outer":[{"x":0,"y":0},{"x":100000000,"y":0},{"x":100000000,"y":100000000},{"x":0,"y":100000000}],"cutouts":[]}),
        )
        .unwrap(),
    );
    for (i, x) in [10_000_123, 30_000_257].into_iter().enumerate() {
        let pad = Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: serde_json::from_value(json!({"x":0,"y":0})).unwrap(),
            width: serde_json::from_value(json!(1000000)).unwrap(),
            height: serde_json::from_value(json!(1000000)).unwrap(),
            net: Some("EXACT".into()),
            apertures: None,
        };
        let part = Part {
            holes: None,
            artwork: None,
            assembly: None,
            schematic_symbol: None,
            schematic_package: None,
            id: ObjectId::new(),
            designator: format!("J{i}"),
            value: String::new(),
            footprint: String::new(),
            position: serde_json::from_value(json!({"x":x,"y":10000129})).unwrap(),
            rotation_mdeg: 0,
            side: if i == 0 { Side::Top } else { Side::Bottom },
            pads: Some(Pads::try_from(vec![pad]).unwrap()),
        };
        project.board.parts.insert(part.id, part);
    }
    let e = EngineHandle::new(project);
    let registry = engine_registry(e.clone()).unwrap();
    let a = CallContext::new("endpoint-a");
    let b = CallContext::new("endpoint-b");
    let before = e.session_state();
    let page = registry.call("query_route_endpoints", json!({"net":"EXACT"}), a.clone()).await.unwrap();
    assert_eq!(page["total"], 2);
    assert_eq!(e.session_state(), before);
    // Discovery order is stable by identity, not by randomly generated UUID's layer.
    let endpoints = page["endpoints"].as_array().unwrap();
    let top = endpoints.iter().find(|endpoint| endpoint["layer"] == "top").unwrap();
    let bottom = endpoints.iter().find(|endpoint| endpoint["layer"] == "bottom").unwrap();
    let input = json!({"expected_project":page["project_id"],"expected_cursor":page["source_cursor"],"start":top["id"],"target":bottom["id"],"via":{"x":20000000,"y":20000000},"before":[],"after":[],"grid":250000,"width":250000,"via_diameter":600000,"via_drill":300000});
    let result = registry.call("route_pad_via_polyline", input.clone(), a.clone()).await.unwrap();
    assert_eq!(result["created"].as_array().unwrap().len(), 3);
    let after = e.session_state();
    let track = e.read(|p| p.board.tracks.values().next().unwrap().clone());
    assert_eq!(e.read(|p| p.board.vias.len()), 1);
    assert!(e.read(|p| p.board.tracks.values().any(|t| t.layer == viamind_doc::CopperLayer::Top)));
    assert!(e.read(|p| p.board.tracks.values().any(|t| t.layer == viamind_doc::CopperLayer::Bottom)));
    assert_eq!(track.net.as_deref(), Some("EXACT"));
    assert!(e.read(|p| p.board.tracks.values().any(|t| t.start.x.0 % 250000 != 0 || t.end.x.0 % 250000 != 0)));
    assert!(registry.call("route_pad_via_polyline", input.clone(), b.clone()).await.unwrap_err().to_string().contains("session changed"));
    assert_eq!(e.session_state(), after);
    registry.call("undo", json!({}), a.clone()).await.unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    registry.call("redo", json!({}), a.clone()).await.unwrap();
    assert_eq!(e.read(Clone::clone), after.project);

    registry.call("undo", json!({}), a.clone()).await.unwrap();
    let baseline = e.session_state();
    let mut checked_input = input;
    checked_input.as_object_mut().unwrap().remove("before");
    checked_input.as_object_mut().unwrap().remove("after");
    checked_input["expected_cursor"] = serde_json::to_value(baseline.cursor).unwrap();
    let plan = registry.call("plan_pad_via_walkaround", checked_input.clone(), a.clone()).await.unwrap();
    assert_eq!(plan["complete_scoped_preflight"], true);
    assert_eq!(plan["start_layer"], "top");
    assert_eq!(plan["target_layer"], "bottom");
    assert_eq!(e.session_state(), baseline);
    let result = registry.call("route_pad_via_walkaround", checked_input.clone(), a.clone()).await.unwrap();
    let committed = e.session_state();
    assert_eq!(committed.cursor.sequence, baseline.cursor.sequence + 1);
    assert_eq!(
        result["created"].as_array().unwrap().len(),
        plan["before"]["path"]["points"].as_array().unwrap().len() + plan["after"]["path"]["points"].as_array().unwrap().len() - 1
    );
    for tool in ["plan_pad_via_walkaround", "route_pad_via_walkaround"] {
        assert!(registry.call(tool, checked_input.clone(), b.clone()).await.unwrap_err().to_string().contains("session changed"));
        assert_eq!(e.session_state(), committed);
    }
    registry.call("undo", json!({}), a.clone()).await.unwrap();
    assert_eq!(e.read(Clone::clone), baseline.project);
    registry.call("redo", json!({}), a).await.unwrap();
    assert_eq!(e.read(Clone::clone), committed.project);
}
#[tokio::test]

async fn exact_posture_routes_share_planner_and_two_agent_cursor_guards() {
    use viamind_doc::{ObjectId, Pad, Pads, Part, Side};
    let mut project = Project::new("MCP explicit endpoints");
    project.board.outline = Some(
        serde_json::from_value(
            json!({"outer":[{"x":0,"y":0},{"x":100000000,"y":0},{"x":100000000,"y":100000000},{"x":0,"y":100000000}],"cutouts":[]}),
        )
        .unwrap(),
    );
    for (i, x) in [10_000_123, 30_000_257].into_iter().enumerate() {
        let pad = Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: serde_json::from_value(json!({"x":0,"y":0})).unwrap(),
            width: serde_json::from_value(json!(1000000)).unwrap(),
            height: serde_json::from_value(json!(1000000)).unwrap(),
            net: Some("EXACT".into()),
            apertures: None,
        };
        let part = Part {
            holes: None,
            artwork: None,
            assembly: None,
            schematic_symbol: None,
            schematic_package: None,
            id: ObjectId::new(),
            designator: format!("J{i}"),
            value: String::new(),
            footprint: String::new(),
            position: serde_json::from_value(json!({"x":x,"y":if i == 0 {10000129} else {20000129}})).unwrap(),
            rotation_mdeg: 0,
            side: Side::Bottom,
            pads: Some(Pads::try_from(vec![pad]).unwrap()),
        };
        project.board.parts.insert(part.id, part);
    }
    let e = EngineHandle::new(project);
    let registry = engine_registry(e.clone()).unwrap();
    let a = CallContext::new("endpoint-a");
    let b = CallContext::new("endpoint-b");
    let before = e.session_state();
    let page = registry.call("query_route_endpoints", json!({"net":"EXACT"}), a.clone()).await.unwrap();
    assert_eq!(page["total"], 2);
    assert_eq!(page["endpoints"][0]["layer"], "bottom");
    assert_eq!(e.session_state(), before);
    let input = json!({"expected_project":page["project_id"],"expected_cursor":page["source_cursor"],"start":page["endpoints"][0]["id"],"target":page["endpoints"][1]["id"],"waypoints":[],"grid":250000,"width":250000,"mode":"octilinear","posture":"alternate"});
    let result = registry.call("route_pad_posture", input.clone(), a.clone()).await.unwrap();
    assert_eq!(result["created"].as_array().unwrap().len(), 4);
    let after = e.session_state();
    let track = e.read(|p| p.board.tracks.values().next().unwrap().clone());
    assert_eq!(track.layer, viamind_doc::CopperLayer::Bottom);
    assert_eq!(track.net.as_deref(), Some("EXACT"));
    assert!(e.read(|p| p.board.tracks.values().any(|t| t.start.x.0 % 250000 != 0)));
    assert!(e.read(|p| p.board.tracks.values().any(|t| t.end.x.0 % 250000 != 0)));
    assert!(registry.call("route_pad_posture", input, b).await.unwrap_err().to_string().contains("session changed"));
    assert_eq!(e.session_state(), after);
    registry.call("undo", json!({}), a.clone()).await.unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    registry.call("redo", json!({}), a).await.unwrap();
    assert_eq!(e.read(Clone::clone), after.project);
}

#[tokio::test]
async fn checked_walkaround_query_preserves_session_and_two_agents_share_atomic_guarded_commit() {
    use viamind_doc::{ObjectId, Pad, Pads, Part, Side};
    let nm = |value: i64| serde_json::from_value(json!(value)).unwrap();
    let point = |x: i64, y: i64| serde_json::from_value(json!({"x":x,"y":y})).unwrap();
    for cutout in [false, true] {
        let cutouts = if cutout {
            json!([[{"x":19000000,"y":8000000},{"x":21000000,"y":8000000},{"x":21000000,"y":12000000},{"x":19000000,"y":12000000}]])
        } else {
            json!([])
        };
        let mut project = Project::new("MCP static detour");
        project.board.outline = Some(
            serde_json::from_value(
                json!({"outer":[{"x":0,"y":0},{"x":40000000,"y":0},{"x":40000000,"y":20000000},{"x":0,"y":20000000}],"cutouts":cutouts}),
            )
            .unwrap(),
        );
        let ids = [10_000_000, 30_000_000].map(|x| {
            let pad = Pad {
                id: ObjectId::new(),
                number: "1".into(),
                position: point(0, 0),
                width: nm(1_000_000),
                height: nm(1_000_000),
                net: Some("N".into()),
                apertures: None,
            };
            let part = Part {
                holes: None,
                artwork: None,
                assembly: None,
                schematic_symbol: None,
                schematic_package: None,
                id: ObjectId::new(),
                designator: format!("J{x}"),
                value: String::new(),
                footprint: String::new(),
                position: point(x, 10_000_000),
                rotation_mdeg: 0,
                side: Side::Top,
                pads: Some(Pads::try_from(vec![pad.clone()]).unwrap()),
            };
            let id = json!({"part_id":part.id,"pad_id":pad.id});
            project.board.parts.insert(part.id, part);
            id
        });
        let e = EngineHandle::new(project);
        let registry = engine_registry(e.clone()).unwrap();
        let a = CallContext::new("walk-a");
        let b = CallContext::new("walk-b");
        if !cutout {
            registry
                .call(
                    "add_track",
                    json!({"start_x_mm":20,"start_y_mm":8,"end_x_mm":20,"end_y_mm":12,"width_mm":0.25,"net":"BLOCK","layer":"top"}),
                    b.clone(),
                )
                .await
                .unwrap();
        }
        let before = e.session_state();
        let input = json!({"expected_project":before.project.id,"expected_cursor":before.cursor,"start":ids[0],"target":ids[1],"grid":1000000,"width":250000});
        let plan = registry.call("plan_pad_walkaround", input.clone(), a.clone()).await.unwrap();
        assert_eq!(e.session_state(), before);
        assert_eq!(plan["complete_scoped_preflight"], true);
        let return_x = if cutout { 22_000_000 } else { 21_000_000 };
        assert_eq!(
            plan["plan"]["path"]["points"],
            json!([{"x":10000000,"y":10000000},{"x":10000000,"y":7000000},{"x":return_x,"y":7000000},{"x":return_x,"y":10000000},{"x":30000000,"y":10000000}])
        );
        let out = registry.call("route_pad_walkaround", input.clone(), a.clone()).await.unwrap();
        assert_eq!(out["created"].as_array().unwrap().len(), 4);
        let after = e.session_state();
        assert!(registry.call("plan_pad_walkaround", input.clone(), b.clone()).await.unwrap_err().to_string().contains("session changed"));
        assert!(registry.call("route_pad_walkaround", input, b).await.unwrap_err().to_string().contains("session changed"));
        assert_eq!(e.session_state(), after);
        registry.call("undo", json!({}), a.clone()).await.unwrap();
        assert_eq!(e.read(Clone::clone), before.project);
        registry.call("redo", json!({}), a).await.unwrap();
        assert_eq!(e.read(Clone::clone), after.project);
    }
}
