//! Grouped placement is atomic, deterministic and preserves local pad identities.
use serde_json::{Value, json};
use viamind_doc::{ObjectId, Project};
use viamind_engine::{EngineHandle, Origin};
fn setup() -> (EngineHandle, Vec<ObjectId>) {
    let e = EngineHandle::new(Project::new("Place"));
    let ids = [0, 1, 2, 10]
        .into_iter()
        .enumerate()
        .map(|(i, x)| {
            e.execute("add_part", json!({"designator":format!("R{i}"),"value":"1k","footprint":"R_0603","x_mm":x,"y_mm":i}), Origin::User)
                .unwrap()
                .created[0]
        })
        .collect::<Vec<_>>();
    let part = e.read(|p| p.board.part(&ids[0]).unwrap().clone());
    e.execute(
        "set_part_pads",
        json!({"id":ids[0],"expected":part,"pads":[{"number":"1","x_mm":0.000001,"y_mm":1,"width_mm":1,"height_mm":1,"net":"GND"}]}),
        Origin::User,
    )
    .unwrap();
    (e, ids)
}
fn input(e: &EngineHandle, ids: &[ObjectId], operation: Value) -> Value {
    e.read(|p| json!({"expected_project":p.id,"expected":ids.iter().map(|id|p.board.part(id).unwrap()).collect::<Vec<_>>(),"operation":operation}))
}
#[test]
fn grouped_moves_preserve_exact_model_geometry_and_undo_as_one_transaction() {
    let (e, ids) = setup();
    let before = e.read(Clone::clone);
    let result = e.execute("transform_parts", input(&e, &ids, json!({"kind":"translate","delta":{"x":1234567,"y":-2000001}})), Origin::User).unwrap();
    assert_eq!(result.deltas.len(), 4);
    let after = e.read(Clone::clone);
    for id in &ids {
        let a = after.board.part(id).unwrap();
        let b = before.board.part(id).unwrap();
        assert_eq!(a.position.x.0, b.position.x.0 + 1234567);
        assert_eq!(a.position.y.0, b.position.y.0 - 2000001);
        assert_eq!(a.pads, b.pads);
        assert_eq!(a.id, b.id);
    }
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before);
    e.redo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), after);
    e.execute("transform_parts", input(&e, &ids, json!({"kind":"rotate","degrees":-90})), Origin::User).unwrap();
    e.execute("transform_parts", input(&e, &ids, json!({"kind":"flip"})), Origin::User).unwrap();
    e.read(|p| {
        for id in &ids {
            let a = p.board.part(id).unwrap();
            assert_eq!(a.rotation_mdeg, 270000);
            assert_eq!(a.side, viamind_doc::Side::Bottom);
            assert_eq!(a.pads, after.board.part(id).unwrap().pads);
        }
    });
}
#[test]
fn stale_member_missing_targets_invalid_bounds_and_duplicates_change_nothing() {
    let (e, ids) = setup();
    let stale = input(&e, &ids, json!({"kind":"translate","delta":{"x":1,"y":1}}));
    e.execute("move_part", json!({"id":ids[2],"x_mm":3,"y_mm":2}), Origin::agent("other")).unwrap();
    let state = e.session_state();
    assert!(e.execute("transform_parts", stale, Origin::User).is_err());
    assert_eq!(e.session_state(), state);
    let mut duplicate = input(&e, &ids, json!({"kind":"flip"}));
    duplicate["expected"][1] = duplicate["expected"][0].clone();
    let mut wrong_project = input(&e, &ids, json!({"kind":"flip"}));
    wrong_project["expected_project"] = json!(ObjectId::new());
    for call in [
        duplicate,
        wrong_project,
        input(&e, &[], json!({"kind":"flip"})),
        input(&e, &ids, json!({"kind":"translate","delta":{"x":i64::MAX,"y":0}})),
        input(&e, &ids, json!({"kind":"translate","delta":{"x":10000000000_i64,"y":0}})),
        input(&e, &ids, json!({"kind":"rotate","degrees":361})),
    ] {
        assert!(e.execute("transform_parts", call, Origin::User).is_err());
        assert_eq!(e.session_state(), state);
    }
    let missing = input(&e, &ids, json!({"kind":"flip"}));
    e.execute("delete_part", json!({"id":ids[1]}), Origin::User).unwrap();
    let state = e.session_state();
    assert!(e.execute("transform_parts", missing, Origin::User).is_err());
    assert_eq!(e.session_state(), state);
}
#[test]
fn origin_distribution_and_alignment_keep_other_axis_and_have_noop_history() {
    let (e, ids) = setup();
    let before = e.read(Clone::clone);
    e.execute("transform_parts", input(&e, &ids, json!({"kind":"distribute","axis":"x"})), Origin::User).unwrap();
    e.read(|p| {
        for (id, x) in ids.iter().zip([0, 3333333, 6666666, 10000000]) {
            let a = p.board.part(id).unwrap();
            assert_eq!(a.position.x.0, x);
            assert_eq!(a.position.y, before.board.part(id).unwrap().position.y);
        }
    });
    let stable = e.session_state();
    let tx = e.next_undo();
    let result = e.execute("transform_parts", input(&e, &ids, json!({"kind":"distribute","axis":"x"})), Origin::User).unwrap();
    assert!(result.deltas.is_empty());
    assert_eq!(e.session_state(), stable);
    assert_eq!(e.next_undo(), tx);
    for (reference, y) in [("minimum", 0), ("maximum", 3000000), ("centre", 1500000)] {
        e.execute("transform_parts", input(&e, &ids, json!({"kind":"align","axis":"y","reference":reference})), Origin::User).unwrap();
        assert!(e.read(|p| ids.iter().all(|id| p.board.part(id).unwrap().position.y.0 == y)));
        e.undo(Origin::User).unwrap();
    }
}
