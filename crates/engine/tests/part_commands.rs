//! add_part, move_part and delete_part: JSON behaviour and apply/invert property tests.

use proptest::prelude::*;
use serde_json::json;
use viamind_doc::{ObjectId, Project, Side};
use viamind_engine::{AddPart, Command, CommandError, CommandRegistry, DeletePart, Delta, EngineError, MovePart, apply_all, invert};
use viamind_geom::{Nm, Point};

fn add(designator: &str, x_mm: f64, y_mm: f64) -> AddPart {
    AddPart {
        expected_project: None,
        designator: designator.into(),
        value: "10k".into(),
        footprint: "R_0603".into(),
        x_mm,
        y_mm,
        rotation_deg: None,
        side: None,
    }
}

#[test]
fn builtin_specs() {
    let specs = CommandRegistry::builtin().specs();
    let names: Vec<_> = specs.iter().map(|spec| spec.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "add_part",
            "add_track",
            "add_via",
            "annotate_schematic",
            "apply_footprint",
            "create_schematic_pcb_component",
            "delete_part",
            "delete_schematic_sheet",
            "delete_track",
            "delete_via",
            "draw_schematic_wire_polyline",
            "edit_schematic_sheet",
            "import_board_outline_dxf",
            "link_schematic_symbol",
            "move_part",
            "place_symbol_template",
            "route_pad_polyline",
            "route_pad_posture",
            "route_pad_via_polyline",
            "route_pad_walkaround",
            "route_polyline",
            "set_board_outline",
            "set_part_pads",
            "synchronize_schematic_pcb",
            "transform_parts",
            "update_part"
        ]
    );
    let add = &specs[0];
    let required: Vec<_> = add.input_schema["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(required, ["designator", "value", "footprint", "x_mm", "y_mm"]);
    let side = add.input_schema["properties"]["side"].to_string();
    assert!(side.contains("\"top\"") && side.contains("\"bottom\""), "{side}");
    for spec in &specs {
        assert_eq!(spec.input_schema["type"], "object", "{}", spec.name);
        assert!(!spec.description.is_empty());
    }
}

#[test]
fn add_move_delete_over_json() {
    let registry = CommandRegistry::builtin();
    let mut project = Project::new("demo");

    let deltas = registry
        .dispatch(
            "add_part",
            json!({ "designator": "R1", "value": "10k", "footprint": "R_0603", "x_mm": 10.0, "y_mm": 5.5, "rotation_deg": -90, "side": "bottom" }),
            &mut project,
        )
        .unwrap();
    let [Delta::Part { id, before: None, after: Some(part) }] = deltas.as_slice() else { panic!("{deltas:?}") };
    assert_eq!(part.position, Point::new(Nm(10_000_000), Nm(5_500_000)));
    assert_eq!(part.rotation_mdeg, 270_000);
    assert_eq!(part.side, Side::Bottom);
    assert_eq!(project.board.part(id), Some(part));

    let deltas = registry.dispatch("move_part", json!({ "id": id, "x_mm": 1, "y_mm": 2 }), &mut project).unwrap();
    assert_eq!(deltas.len(), 1);
    assert_eq!(project.board.part(id).unwrap().position, Point::from_mm(1.0, 2.0));
    let deltas = registry.dispatch("move_part", json!({ "id": id, "x_mm": 1, "y_mm": 2 }), &mut project).unwrap();
    assert!(deltas.is_empty(), "moving to the same place changes nothing");

    let deltas = registry.dispatch("delete_part", json!({ "id": id }), &mut project).unwrap();
    assert!(deltas[0].is_delete());
    assert!(project.board.parts.is_empty());
}

#[test]
fn errors_are_actionable_and_change_nothing() {
    let registry = CommandRegistry::builtin();
    let mut project = Project::new("demo");
    let r1 = add("R1", 0.0, 0.0).apply(&mut project).unwrap()[0].id();
    let before = project.clone();
    let missing = ObjectId::new();

    let err = registry.dispatch("move_part", json!({ "id": missing, "x_mm": 0, "y_mm": 0 }), &mut project).unwrap_err();
    assert_eq!(err.to_string(), format!("`move_part` failed: no part with id {missing} exists"));
    let err = registry.dispatch("delete_part", json!({ "id": missing }), &mut project).unwrap_err();
    assert!(matches!(&err, EngineError::Command { error: CommandError::NotFound { .. }, .. }), "{err:?}");

    let err = add(" R1 ", 1.0, 1.0).apply(&mut project).unwrap_err();
    assert_eq!(err.to_string(), format!("designator `R1` is already used by part {r1}; choose another designator or move that part instead"));
    assert!(add("", 0.0, 0.0).apply(&mut project).is_err());
    assert!(add("R2", f64::NAN, 0.0).apply(&mut project).is_err());
    assert!(AddPart { footprint: " ".into(), ..add("R2", 0.0, 0.0) }.apply(&mut project).is_err());
    assert!(AddPart { rotation_deg: Some(f64::INFINITY), ..add("R2", 0.0, 0.0) }.apply(&mut project).is_err());
    assert!(MovePart { id: r1, x_mm: 1e9, y_mm: 0.0 }.apply(&mut project).is_err());

    let err = registry.dispatch("move_part", json!({ "id": "R1", "x_mm": 0, "y_mm": 0 }), &mut project).unwrap_err();
    assert!(matches!(err, EngineError::InvalidInput { .. }), "a designator is not an id");
    let err = registry.dispatch("move_part", json!({ "id": r1, "dx": 1 }), &mut project).unwrap_err();
    assert!(err.to_string().contains("dx"), "{err}");

    assert_eq!(project, before);
}

/// One command to try against a generated board.
#[derive(Clone, Debug)]
enum Op {
    Add(AddPart),
    /// Move the part at this index (modulo the part count), or a missing id when the board is empty.
    Move(usize, f64, f64),
    Delete(usize),
    MoveMissing,
    DeleteMissing,
}

fn arb_add() -> impl Strategy<Value = AddPart> {
    (
        "[A-Z]{1,2}[1-9][0-9]{0,2}",
        "[0-9]{0,3}[kMnpu]?",
        prop::sample::select(vec!["R_0603", "C_0402", "SOIC-8"]),
        -200.0..200.0f64,
        -200.0..200.0f64,
        prop::option::of(-720.0..720.0f64),
        prop::option::of(prop_oneof![Just(Side::Top), Just(Side::Bottom)]),
    )
        .prop_map(|(designator, value, footprint, x_mm, y_mm, rotation_deg, side)| AddPart {
            expected_project: None,
            designator,
            value,
            footprint: footprint.into(),
            x_mm,
            y_mm,
            rotation_deg,
            side,
        })
}

fn arb_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        arb_add().prop_map(Op::Add),
        (any::<usize>(), -200.0..200.0f64, -200.0..200.0f64).prop_map(|(i, x, y)| Op::Move(i, x, y)),
        any::<usize>().prop_map(Op::Delete),
        Just(Op::MoveMissing),
        Just(Op::DeleteMissing),
    ]
}

fn arb_project() -> impl Strategy<Value = Project> {
    prop::collection::vec(arb_add(), 0..12).prop_map(|adds| {
        let mut project = Project::new("prop");
        for add in adds {
            let _ = add.apply(&mut project); // duplicate designators are rejected; that is fine here
        }
        project
    })
}

fn pick(project: &Project, index: usize) -> ObjectId {
    let ids: Vec<_> = project.board.parts.keys().copied().collect();
    if ids.is_empty() { ObjectId::new() } else { ids[index % ids.len()] }
}

fn run(op: Op, project: &mut Project) -> Result<Vec<Delta>, CommandError> {
    match op {
        Op::Add(add) => add.apply(project),
        Op::Move(i, x_mm, y_mm) => MovePart { id: pick(project, i), x_mm, y_mm }.apply(project),
        Op::Delete(i) => DeletePart { id: pick(project, i) }.apply(project),
        Op::MoveMissing => MovePart { id: ObjectId::new(), x_mm: 0.0, y_mm: 0.0 }.apply(project),
        Op::DeleteMissing => DeletePart { id: ObjectId::new() }.apply(project),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// For every command: apply then invert restores the project exactly, re-applying the
    /// deltas reproduces the edited project, and a rejected command changes nothing.
    #[test]
    fn apply_invert_round_trip(mut project in arb_project(), ops in prop::collection::vec(arb_op(), 1..8)) {
        for op in ops {
            let before = project.clone();
            match run(op, &mut project) {
                Ok(deltas) => {
                    prop_assert!(deltas.iter().all(|d| !d.is_noop()));
                    let after = project.clone();
                    let mut undone = after.clone();
                    apply_all(&mut undone, &invert(&deltas));
                    prop_assert_eq!(&undone, &before);
                    apply_all(&mut undone, &deltas);
                    prop_assert_eq!(&undone, &after);
                }
                Err(_) => prop_assert_eq!(&project, &before),
            }
            for (id, part) in &project.board.parts {
                prop_assert_eq!(*id, part.id);
                prop_assert!((0..360_000).contains(&part.rotation_mdeg));
            }
        }
    }

    /// JSON input built from a typed command dispatches to the same edit.
    #[test]
    fn json_and_typed_agree(add in arb_add()) {
        let registry = CommandRegistry::builtin();
        let mut by_json = Project::new("p");
        let mut typed = by_json.clone();
        let json_deltas = registry.dispatch("add_part", serde_json::to_value(&add).unwrap(), &mut by_json).unwrap();
        let typed_deltas = add.apply(&mut typed).unwrap();
        // Ids are random per call, so compare everything but the id.
        let strip = |deltas: &[Delta]| match &deltas[0] {
            Delta::Part { after: Some(part), .. } => (part.designator.clone(), part.position, part.rotation_mdeg, part.side),
            other => panic!("{other:?}"),
        };
        prop_assert_eq!(strip(&json_deltas), strip(&typed_deltas));
    }
}
