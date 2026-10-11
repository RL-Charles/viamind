use serde_json::json;
use viamind_doc::{BoardOutline, Project};
use viamind_engine::EngineHandle;
use viamind_mcp::{CallContext, engine_registry};
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
        };
        let part = Part {
            schematic_symbol: None,
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
        };
        let part = Part {
            schematic_symbol: None,
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
    let input = json!({"expected_project":page["project_id"],"expected_cursor":page["source_cursor"],"start":page["endpoints"][0]["id"],"target":page["endpoints"][1]["id"],"via":{"x":20000000,"y":20000000},"before":[],"after":[],"grid":250000,"width":250000,"via_diameter":600000,"via_drill":300000});
    let result = registry.call("route_pad_via_polyline", input.clone(), a.clone()).await.unwrap();
    assert_eq!(result["created"].as_array().unwrap().len(), 3);
    let after = e.session_state();
    let track = e.read(|p| p.board.tracks.values().next().unwrap().clone());
    assert_eq!(e.read(|p| p.board.vias.len()), 1);
    assert!(e.read(|p| p.board.tracks.values().any(|t| t.layer == viamind_doc::CopperLayer::Top)));
    assert!(e.read(|p| p.board.tracks.values().any(|t| t.layer == viamind_doc::CopperLayer::Bottom)));
    assert_eq!(track.net.as_deref(), Some("EXACT"));
    assert!(e.read(|p| p.board.tracks.values().any(|t| t.start.x.0 % 250000 != 0 || t.end.x.0 % 250000 != 0)));
    assert!(registry.call("route_pad_via_polyline", input, b).await.unwrap_err().to_string().contains("session changed"));
    assert_eq!(e.session_state(), after);
    registry.call("undo", json!({}), a.clone()).await.unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    registry.call("redo", json!({}), a).await.unwrap();
    assert_eq!(e.read(Clone::clone), after.project);
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
        };
        let part = Part {
            schematic_symbol: None,
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
    use viamind_geom::{Nm, Point};
    let mut project = Project::new("MCP static detour");
    project.board.outline = Some(
        serde_json::from_value(json!({"outer":[{"x":0,"y":0},{"x":40000000,"y":0},{"x":40000000,"y":20000000},{"x":0,"y":20000000}],"cutouts":[]}))
            .unwrap(),
    );
    let ids = [10_000_000, 30_000_000].map(|x| {
        let pad = Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: Point::ORIGIN,
            width: Nm(1_000_000),
            height: Nm(1_000_000),
            net: Some("N".into()),
        };
        let part = Part {
            schematic_symbol: None,
            id: ObjectId::new(),
            designator: format!("J{x}"),
            value: String::new(),
            footprint: String::new(),
            position: Point::new(Nm(x), Nm(10_000_000)),
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
    registry
        .call("add_track", json!({"start_x_mm":20,"start_y_mm":8,"end_x_mm":20,"end_y_mm":12,"width_mm":0.25,"net":"BLOCK","layer":"top"}), b.clone())
        .await
        .unwrap();
    let before = e.session_state();
    let input =
        json!({"expected_project":before.project.id,"expected_cursor":before.cursor,"start":ids[0],"target":ids[1],"grid":1000000,"width":250000});
    let plan = registry.call("plan_pad_walkaround", input.clone(), a.clone()).await.unwrap();
    assert_eq!(e.session_state(), before);
    assert_eq!(plan["complete_scoped_preflight"], true);
    assert_eq!(
        plan["plan"]["path"]["points"],
        json!([{"x":10000000,"y":10000000},{"x":10000000,"y":7000000},{"x":21000000,"y":7000000},{"x":21000000,"y":10000000},{"x":30000000,"y":10000000}])
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
