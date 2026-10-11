//! The native format's ordering is structural: the same project serialises to the
//! same bytes no matter in which order its objects were inserted
//! (docs/decisions/0001-file-format.md).

use viamind_doc::{ObjectId, Part, Project, Sheet, Side};
use viamind_geom::Point;

fn parts() -> Vec<Part> {
    (0..200)
        .map(|i| Part {
            id: ObjectId::new(),
            designator: format!("R{i}"),
            value: "10k".into(),
            footprint: "R_0603".into(),
            position: Point::from_mm(f64::from(i) * 1.27, -3.5),
            rotation_mdeg: (i * 45_000) % 360_000,
            side: if i % 2 == 0 { Side::Top } else { Side::Bottom },
        })
        .collect()
}

fn build(template: &Project, sheets: &[Sheet], parts: impl Iterator<Item = Part>) -> Project {
    let mut project = template.clone();
    for sheet in sheets.iter().rev() {
        project.schematic.sheets.insert(sheet.id, sheet.clone());
    }
    for part in parts {
        project.board.parts.insert(part.id, part);
    }
    project
}

#[test]
fn insertion_order_does_not_change_the_bytes() {
    let template = Project::new("determinism");
    let sheets = [Sheet::new("Power"), Sheet::new("MCU"), Sheet::new("IO")];
    let parts = parts();

    let forward = build(&template, &sheets, parts.iter().cloned());
    let reverse = build(&template, &sheets, parts.iter().rev().cloned());
    let interleaved = build(&template, &sheets, parts.iter().step_by(2).chain(parts.iter().skip(1).step_by(2)).cloned());

    let bytes = serde_json::to_string_pretty(&forward).unwrap();
    assert_eq!(serde_json::to_string_pretty(&reverse).unwrap(), bytes);
    assert_eq!(serde_json::to_string_pretty(&interleaved).unwrap(), bytes);

    // Re-serialising what was read back gives the same bytes again.
    let reread: Project = serde_json::from_str(&bytes).unwrap();
    assert_eq!(serde_json::to_string_pretty(&reread).unwrap(), bytes);

    // Objects appear sorted by id.
    let order: Vec<_> = parts.iter().map(|p| p.id).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
    let positions: Vec<_> = order.iter().map(|id| bytes.find(&format!("\"{id}\": {{")).unwrap()).collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]));
}
