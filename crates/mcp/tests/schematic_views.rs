use serde_json::json;
use viamind_engine::{EngineHandle, Origin};
use viamind_mcp::{agent::CallContext, engine_registry};
#[tokio::test]
async fn selected_sheet_views_are_guarded_bounded_and_immutable() {
    let p = viamind_doc::file::from_str(include_str!("../../../fixtures/views/schematic.viamind.json")).unwrap();
    let sheet = *p.schematic.sheets.keys().next().unwrap();
    let engine = EngineHandle::new(p);
    let tools = engine_registry(engine.clone()).unwrap();
    let agent = CallContext::new("schematic-reader");
    let before = engine.session_state();
    let status = engine.document_status();
    let dir = tempfile::tempdir().unwrap();
    for tool in ["render_schematic_view", "render_schematic_png"] {
        let path = dir.path().join(tool);
        let result = tools
            .call(tool, json!({"sheet":sheet,"expected_cursor":before.cursor,"path":path,"options":{"width_px":300,"height_px":200}}), agent.clone())
            .await
            .unwrap();
        assert_eq!(result["source_cursor"], json!(before.cursor));
        assert_eq!(result["metadata"]["sheet_id"], json!(sheet));
        assert_eq!(result["metadata"]["pin_count"], 2);
        assert_eq!(engine.session_state(), before);
        assert_eq!(engine.document_status(), status);
        assert!(tools.call(tool, json!({"sheet":sheet,"path":path}), agent.clone()).await.is_err());
        for input in [
            json!({}),
            json!({"sheet":sheet,"overwrite":true}),
            json!({"sheet":sheet,"options":{"width_px":4096,"height_px":4096}}),
            json!({"sheet":sheet,"options":{"layers":["top_copper"]}}),
        ] {
            assert!(tools.call(tool, input, agent.clone()).await.is_err());
        }
    }
    engine.execute("add_via", json!({"x_mm":2,"y_mm":3,"diameter_mm":1,"drill_mm":0.4}), Origin::agent("other")).unwrap();
    for tool in ["render_schematic_view", "render_schematic_png"] {
        let path = dir.path().join(format!("stale-{tool}"));
        assert!(tools.call(tool, json!({"sheet":sheet,"expected_cursor":before.cursor,"path":path}), agent.clone()).await.is_err());
        assert!(!path.exists());
    }
    let fresh = engine.session_state();
    let view = tools.call("render_schematic_view", json!({"sheet":sheet,"expected_cursor":fresh.cursor}), agent).await.unwrap();
    assert_eq!(view["source_cursor"], json!(fresh.cursor));
    assert_eq!(engine.session_state(), fresh);
}
#[tokio::test]
async fn unrelated_giant_content_is_excluded_and_selected_budget_rejects_unchanged() {
    let mut p = viamind_doc::file::from_str(include_str!("../../../fixtures/views/schematic.viamind.json")).unwrap();
    let sheet = *p.schematic.sheets.keys().next().unwrap();
    let mut other = viamind_doc::Sheet::new("X".repeat(9 * 1024 * 1024));
    other.id = p.id;
    p.schematic.sheets.insert(viamind_doc::ObjectId::new(), other);
    let engine = EngineHandle::new(p);
    let tools = engine_registry(engine.clone()).unwrap();
    let cursor = engine.with_engine(viamind_engine::Engine::event_cursor);
    let status = engine.document_status();
    let agent = CallContext::new("bounded-reader");
    for tool in ["render_schematic_view", "render_schematic_png"] {
        let result =
            tools.call(tool, json!({"sheet":sheet,"expected_cursor":cursor,"options":{"width_px":64,"height_px":64}}), agent.clone()).await.unwrap();
        assert_eq!(result["metadata"]["pin_count"], 2);
    }
    assert_eq!(engine.with_engine(viamind_engine::Engine::event_cursor), cursor);
    assert_eq!(engine.document_status(), status);
    let mut p = engine.session_state().project;
    p.schematic.sheets.get_mut(&sheet).unwrap().name = "X".repeat(4097);
    let engine = EngineHandle::new(p);
    let tools = engine_registry(engine.clone()).unwrap();
    let before = engine.session_state();
    let dir = tempfile::tempdir().unwrap();
    for tool in ["render_schematic_view", "render_schematic_png"] {
        let path = dir.path().join(tool);
        assert!(tools.call(tool, json!({"sheet":sheet,"path":path}), agent.clone()).await.is_err());
        assert!(!path.exists());
        assert_eq!(engine.session_state(), before);
    }
}
