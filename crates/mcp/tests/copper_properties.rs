use serde_json::json;
use viamind_doc::{ObjectId, Project, Track, Via};
use viamind_engine::EngineHandle;
use viamind_mcp::{CallContext, engine_registry};
#[tokio::test]
async fn two_clients_and_read_only_observer_share_guarded_stable_property_commands() {
    let mut project = Project::new("properties parity");
    project.board.outline = Some(
        serde_json::from_value(json!({"outer":[{"x":0,"y":0},{"x":50000000,"y":0},{"x":50000000,"y":50000000},{"x":0,"y":50000000}],"cutouts":[]}))
            .unwrap(),
    );
    let t: Track = serde_json::from_value(
        json!({"id":ObjectId::new(),"start":{"x":5000003,"y":10000007},"end":{"x":15000001,"y":10000007},"width":250000,"layer":"top","net":"N"}),
    )
    .unwrap();
    let v: Via =
        serde_json::from_value(json!({"id":ObjectId::new(),"position":{"x":30000001,"y":25000003},"diameter":1000000,"drill":300000,"net":"N"}))
            .unwrap();
    project.board.tracks.insert(t.id, t.clone());
    project.board.vias.insert(v.id, v.clone());
    let e = EngineHandle::new(project);
    let registry = engine_registry(e.clone()).unwrap();
    let read = registry.clone().read_only();
    let a = CallContext::new("property-a");
    let b = CallContext::new("property-b");
    let old = e.session_state();
    let status = e.document_status();
    let query = json!({"target":{"kind":"track","id":t.id}});
    let observed = read.call("query_copper_properties", query.clone(), b.clone()).await.unwrap();
    assert_eq!(observed["source_cursor"], serde_json::to_value(old.cursor).unwrap());
    assert_eq!(observed["object"]["track"], serde_json::to_value(&t).unwrap());
    assert_eq!(e.session_state(), old);
    assert_eq!(e.document_status(), status);
    let request =
        json!({"expected_project":old.project.id,"expected_cursor":old.cursor,"id":t.id,"expected":t,"width":350001,"layer":"bottom","net":"N"});
    assert!(read.call("set_track_properties", request.clone(), b.clone()).await.is_err());
    assert_eq!(e.session_state(), old);
    registry.call("set_track_properties", request.clone(), a.clone()).await.unwrap();
    let now = e.session_state();
    assert_eq!(now.project.board.tracks[&t.id].width.0, 350001);
    assert_eq!(now.project.board.tracks[&t.id].start, t.start);
    assert_eq!(now.cursor.sequence, old.cursor.sequence + 1);
    let peer = registry.call("query_copper_properties", query, b.clone()).await.unwrap();
    assert_eq!(peer["object"]["track"], serde_json::to_value(&now.project.board.tracks[&t.id]).unwrap());
    assert!(registry.call("set_track_properties", request, b.clone()).await.is_err());
    assert_eq!(e.session_state(), now);
    let request =
        json!({"expected_project":now.project.id,"expected_cursor":now.cursor,"id":v.id,"expected":v,"diameter":900003,"drill":400001,"net":"N"});
    assert!(read.call("set_via_properties", request.clone(), b.clone()).await.is_err());
    registry.call("set_via_properties", request, a.clone()).await.unwrap();
    let after = e.session_state();
    assert_eq!(after.project.board.vias[&v.id].position, v.position);
    assert_eq!(after.project.board.vias[&v.id].diameter.0, 900003);
    assert!(registry.call("undo", json!({}), b).await.is_err());
    assert_eq!(e.session_state(), after);
    registry.call("undo", json!({}), a.clone()).await.unwrap();
    assert_eq!(e.read(Clone::clone), now.project);
    let query = json!({"target":{"kind":"via","id":v.id,"position":{"x":1,"y":1}}});
    let before = e.session_state();
    assert!(read.call("query_copper_properties", query, a).await.is_err());
    assert_eq!(e.session_state(), before);
}
