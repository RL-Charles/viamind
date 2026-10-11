use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use serde_json::json;
use viamind_doc::{ObjectId, Part, Project, Side};
use viamind_engine::{EngineHandle, Origin, PartQuery, SelectionMode, SetSelection};
use viamind_geom::Point;

fn fixture() -> EngineHandle {
    let mut project = Project::new("query fixture");
    for (number, designator, value, footprint, x, y, side) in [
        (1, "R1", "10k", "R_0603", 1.000001, -2.0, Side::Top),
        (2, "R2", "10k", "R_0603", 2.0, -2.0, Side::Bottom),
        (3, "C1", "100nF", "C_0603", 5.0, 3.0, Side::Top),
    ] {
        let id = format!("00000000-0000-4000-8000-{number:012}").parse().unwrap();
        project.board.parts.insert(
            id,
            Part {
                artwork: None,
                assembly: None,
                schematic_symbol: None,
                schematic_package: None,
                pads: None,
                id,
                designator: designator.into(),
                value: value.into(),
                footprint: footprint.into(),
                position: Point::from_mm(x, y),
                rotation_mdeg: 0,
                side,
            },
        );
    }
    EngineHandle::new(project)
}

fn query(engine: &EngineHandle, input: serde_json::Value) -> Vec<String> {
    let query: PartQuery = serde_json::from_value(input).unwrap();
    engine.query_parts(&query).unwrap().parts.into_iter().map(|part| part.designator).collect()
}

#[test]
fn filters_return_known_records_and_combine_with_and() {
    let engine = fixture();
    assert_eq!(query(&engine, json!({})), ["R1", "R2", "C1"]);
    assert_eq!(query(&engine, json!({ "value": "10k" })), ["R1", "R2"]);
    assert_eq!(query(&engine, json!({ "footprint": "C_0603" })), ["C1"]);
    assert_eq!(query(&engine, json!({ "designator": "R1" })), ["R1"]);
    assert!(query(&engine, json!({ "designator": "r1" })).is_empty());
    assert!(query(&engine, json!({ "value": "10K" })).is_empty());
    assert_eq!(query(&engine, json!({ "text": "10K", "side": "bottom" })), ["R2"]);
    assert_eq!(query(&engine, json!({ "text": "c_06" })), ["C1"]);
    assert_eq!(query(&engine, json!({ "text": "r1" })), ["R1"]);
    assert_eq!(query(&engine, json!({ "text": "", "side": "top" })), ["R1", "C1"]);
    let r1 = engine.read(|p| p.board.part_by_designator("R1").unwrap().id);
    assert_eq!(query(&engine, json!({ "ids": [r1, r1, ObjectId::new()] })), ["R1"]);
    assert!(query(&engine, json!({ "ids": [] })).is_empty());
    assert!(query(&engine, json!({ "ids": [r1], "side": "bottom" })).is_empty());
    assert_eq!(
        query(
            &engine,
            json!({
                "ids": [r1], "designator": "R1", "value": "10k", "footprint": "R_0603", "text": "r_06", "side": "top",
                "bounds": { "min_x_mm": 1.000001, "min_y_mm": -2, "max_x_mm": 2, "max_y_mm": -2 }
            })
        ),
        ["R1"]
    );
    assert!(
        query(
            &engine,
            json!({
                "bounds": { "min_x_mm": 1.000002, "min_y_mm": -2, "max_x_mm": 1.5, "max_y_mm": -2 }
            })
        )
        .is_empty()
    );
    assert_eq!(
        query(
            &engine,
            json!({
                "bounds": { "min_x_mm": 1.0000006, "min_y_mm": -2, "max_x_mm": 1.0000014, "max_y_mm": -2 }
            })
        ),
        ["R1"],
        "bounds round to integer nanometres like placement"
    );
}

#[test]
fn cursor_pages_have_no_duplicates_or_omissions_and_keep_total() {
    let engine = fixture();
    let mut request = PartQuery { limit: 1, ..PartQuery::default() };
    let mut found = Vec::new();
    for expected in ["R1", "R2", "C1"] {
        let page = engine.query_parts(&request).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.parts.len(), 1);
        assert_eq!(page.parts[0].designator, expected);
        assert_eq!(page.next_after.is_some(), expected != "C1");
        found.push(page.parts[0].id);
        request.after = Some(page.parts[0].id);
    }
    let end = engine.query_parts(&request).unwrap();
    assert!(end.parts.is_empty());
    assert_eq!(end.next_after, None);
    assert_eq!(end.total, 3);
    engine.execute("delete_part", json!({ "id": found[0] }), Origin::User).unwrap();
    assert_eq!(query(&engine, json!({ "after": found[0], "limit": 1 })), ["R2"], "deleted cursor still works");
    let page = engine.query_parts(&PartQuery { value: Some("10k".into()), limit: 1, ..PartQuery::default() }).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.next_after, None);
    assert_eq!(page.parts[0].designator, "R2");
}

#[test]
fn default_page_is_bounded_on_large_designs() {
    let mut project = Project::new("large");
    for number in 0..205 {
        let id = ObjectId::new();
        project.board.parts.insert(
            id,
            Part {
                artwork: None,
                assembly: None,
                schematic_symbol: None,
                schematic_package: None,
                pads: None,
                id,
                designator: format!("R{number}"),
                value: "1k".into(),
                footprint: "R_0603".into(),
                position: Point::ORIGIN,
                rotation_mdeg: 0,
                side: Side::Top,
            },
        );
    }
    let all: Vec<_> = project.board.parts.values().cloned().collect();
    let engine = EngineHandle::new(project);
    let mut request: PartQuery = serde_json::from_value(json!({})).unwrap();
    let mut found = Vec::new();
    for count in [100, 100, 5] {
        let page = engine.query_parts(&request).unwrap();
        assert_eq!(page.total, 205);
        assert_eq!(page.parts.len(), count);
        request.after = page.next_after;
        found.extend(page.parts);
    }
    assert_eq!(request.after, None);
    assert_eq!(found, all);
}

#[test]
fn invalid_input_is_actionable_and_queries_preserve_all_state() {
    let engine = fixture();
    let r1 = engine.read(|p| p.board.part_by_designator("R1").unwrap().id);
    engine.execute("move_part", json!({ "id": r1, "x_mm": 8, "y_mm": 9 }), Origin::User).unwrap();
    engine.undo(Origin::User).unwrap();
    engine.set_selection(SetSelection { ids: vec![r1], mode: SelectionMode::Replace }, Origin::User).unwrap();
    let project = engine.read(Clone::clone);
    let selection = engine.selection();
    let log = engine.with_engine(|e| serde_json::to_value(e.log().entries()).unwrap());
    let undo = engine.next_undo();
    let redo = engine.next_redo();
    let events = engine.subscribe();
    let wakes = Arc::new(AtomicUsize::new(0));
    let count = wakes.clone();
    engine.on_change(move || {
        count.fetch_add(1, Ordering::SeqCst);
    });
    engine.query_parts(&PartQuery::default()).unwrap();
    for (input, message) in [
        (json!({ "limit": 0 }), "limit"),
        (json!({ "limit": 1001 }), "limit"),
        (json!({ "bounds": { "min_x_mm": 2, "min_y_mm": 0, "max_x_mm": 1, "max_y_mm": 1 } }), "bounds"),
        (json!({ "bounds": { "min_x_mm": 0, "min_y_mm": 0, "max_x_mm": 10001, "max_y_mm": 1 } }), "bounds.max_x_mm"),
    ] {
        let request: PartQuery = serde_json::from_value(input).unwrap();
        assert!(engine.query_parts(&request).unwrap_err().to_string().contains(message));
    }
    for input in [
        json!({ "limit": -1 }),
        json!({ "side": "front" }),
        json!({ "ids": ["bad-uuid"] }),
        json!({ "unknown": true }),
        json!({ "bounds": { "min_x_mm": 0, "extra": 1 } }),
    ] {
        assert!(serde_json::from_value::<PartQuery>(input).is_err());
    }
    for value in [f64::INFINITY, f64::NAN, -10001.0] {
        let request = PartQuery {
            bounds: Some(viamind_engine::PartQueryBounds { min_x_mm: value, min_y_mm: 0.0, max_x_mm: 1.0, max_y_mm: 1.0 }),
            ..PartQuery::default()
        };
        assert!(engine.query_parts(&request).is_err());
    }
    assert_eq!(engine.read(Clone::clone), project);
    assert_eq!(engine.selection(), selection);
    assert_eq!(engine.with_engine(|e| serde_json::to_value(e.log().entries()).unwrap()), log);
    assert_eq!(engine.next_undo(), undo);
    assert_eq!(engine.next_redo(), redo);
    assert!(events.try_recv().is_err());
    assert_eq!(wakes.load(Ordering::SeqCst), 0);
    engine.redo(Origin::User).unwrap();
    assert_eq!(engine.read(|p| p.board.part(&r1).unwrap().position), Point::from_mm(8.0, 9.0));
}
