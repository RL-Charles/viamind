use serde_json::json;
use viamind_engine::{EngineHandle, Origin};
use viamind_mcp::{agent::CallContext, engine_registry};
#[tokio::test]
async fn views_return_exact_read_only_cursor_and_shared_bytes_with_stale_guards() {
    let p = viamind_doc::file::from_str(include_str!("../../../fixtures/views/asymmetric.viamind.json")).unwrap();
    let engine = EngineHandle::new(p);
    let tools = engine_registry(engine.clone()).unwrap();
    let a = CallContext::new("view-agent");
    let before = engine.session_state();
    let status = engine.document_status();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("board.svg");
    let result = tools
        .call("render_board_view", json!({"path":path,"expected_cursor":before.cursor,"options":{"layers":["top_copper"],"width_px":800}}), a.clone())
        .await
        .unwrap();
    assert_eq!(result["media_type"], "image/svg+xml");
    assert_eq!(result["metadata"]["pad_count"], 1);
    assert_eq!(result["source_cursor"], json!(before.cursor));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), result["svg"].as_str().unwrap());
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    assert!(tools.call("render_board_view", json!({"path":path}), a.clone()).await.is_err());
    engine.execute("add_via", json!({"x_mm":4,"y_mm":5,"diameter_mm":1,"drill_mm":0.5}), Origin::agent("other")).unwrap();
    let stale = dir.path().join("stale.svg");
    assert!(tools.call("render_board_view", json!({"path":stale,"expected_cursor":before.cursor}), a.clone()).await.is_err());
    assert!(!stale.exists());
    let fresh = engine.session_state();
    let view = tools.call("render_board_view", json!({"expected_cursor":fresh.cursor}), a.clone()).await.unwrap();
    assert_eq!(view["metadata"]["through_via_count"], 3);
    assert!(view["path"].is_null());
    assert_eq!(engine.session_state(), fresh);
    for input in [
        json!({"overwrite":true}),
        json!({"options":{"width_px":5000}}),
        json!({"options":{"layers":["inner_copper"]}}),
        json!({"options":{"unexpected":1}}),
    ] {
        assert!(tools.call("render_board_view", input, a.clone()).await.is_err());
    }
}
