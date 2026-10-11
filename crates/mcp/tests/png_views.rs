use serde_json::json;
use viamind_engine::{EngineHandle, Origin};
use viamind_mcp::{agent::CallContext, engine_registry};
#[tokio::test]
async fn png_views_return_exact_read_only_cursor_and_shared_bytes_with_stale_guards() {
    let p = viamind_doc::file::from_str(include_str!("../../../fixtures/views/asymmetric.viamind.json")).unwrap();
    let engine = EngineHandle::new(p);
    let tools = engine_registry(engine.clone()).unwrap();
    let a = CallContext::new("view-agent");
    let before = engine.session_state();
    let status = engine.document_status();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.png");
    let result = tools
        .call("render_board_png", json!({"path":path,"expected_cursor":before.cursor,"options":{"layers":["top_copper"],"width_px":800}}), a.clone())
        .await
        .unwrap();
    assert_eq!(result["media_type"], "image/png");
    assert_eq!(result["metadata"]["pad_count"], 1);
    assert_eq!(result["source_cursor"], json!(before.cursor));
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD.decode(result["png_base64"].as_str().unwrap()).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert!(tools.call("render_board_png", json!({"path":path}), a.clone()).await.is_err());
    engine.execute("add_via", json!({"x_mm":4,"y_mm":5,"diameter_mm":1,"drill_mm":0.5}), Origin::agent("other")).unwrap();
    let stale = dir.path().join("stale.png");
    assert!(tools.call("render_board_png", json!({"path":stale,"expected_cursor":before.cursor}), a.clone()).await.is_err());
    assert!(!stale.exists());
    let fresh = engine.session_state();
    let view = tools.call("render_board_png", json!({"expected_cursor":fresh.cursor}), a.clone()).await.unwrap();
    assert_eq!(view["metadata"]["through_via_count"], 3);
    assert!(view["path"].is_null());
    assert_eq!(engine.session_state(), fresh);
    for input in [
        json!({"overwrite":true}),
        json!({"options":{"width_px":4096,"height_px":4096}}),
        json!({"options":{"width_px":5000}}),
        json!({"options":{"layers":["inner_copper"]}}),
        json!({"options":{"unexpected":1}}),
    ] {
        assert!(tools.call("render_board_png", input, a.clone()).await.is_err());
    }
}

#[tokio::test]
async fn svg_and_png_capture_only_board_and_reject_aggregate_budget_without_state_changes() {
    let mut p = viamind_doc::file::from_str(include_str!("../../../fixtures/views/asymmetric.viamind.json")).unwrap();
    let mut sheet = viamind_doc::Sheet::new("Unrelated");
    sheet.name = "unrelated".repeat(1024 * 1024);
    p.schematic.sheets.insert(sheet.id, sheet);
    let engine = EngineHandle::new(p);
    let tools = engine_registry(engine.clone()).unwrap();
    let a = CallContext::new("bounded-reader");
    let cursor = engine.with_engine(|e| e.event_cursor());
    let before_status = engine.document_status();
    for tool in ["render_board_view", "render_board_png"] {
        let result = tools.call(tool, json!({"expected_cursor":cursor,"options":{"width_px":32,"height_px":32}}), a.clone()).await.unwrap();
        assert_eq!(result["source_cursor"], json!(cursor));
        assert_eq!(result["metadata"]["pad_count"], 3);
    }
    assert_eq!(engine.document_status(), before_status);
    assert_eq!(engine.with_engine(|e| e.event_cursor()), cursor);
    assert_eq!(engine.read(|p| p.schematic.sheets.values().next().unwrap().name.len()), 9 * 1024 * 1024);
    let mut p = engine.read(viamind_render::board_view_snapshot).unwrap();
    let mut part = p.board.parts.values().next().unwrap().clone();
    part.pads = None;
    part.value = "v".repeat(4096);
    p.board.parts.clear();
    for _ in 0..513 {
        part.id = viamind_doc::ObjectId::new();
        p.board.parts.insert(part.id, part.clone());
    }
    let engine = EngineHandle::new(p);
    let tools = engine_registry(engine.clone()).unwrap();
    let before = engine.session_state();
    let status = engine.document_status();
    let dir = tempfile::tempdir().unwrap();
    for tool in ["render_board_view", "render_board_png"] {
        let path = dir.path().join(tool);
        let err = tools.call(tool, json!({"path":path}), a.clone()).await.unwrap_err();
        assert!(err.to_string().contains("aggregate"));
        assert!(!path.exists());
        assert_eq!(engine.session_state(), before);
        assert_eq!(engine.document_status(), status);
    }
}
