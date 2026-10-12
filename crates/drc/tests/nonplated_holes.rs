//! Mechanical drills are neither copper nor vias, including pads-None footprints.
use viamind_doc::footprint_holes::PartHole;
use viamind_doc::{BoardOutline, CopperLayer, ObjectId, Pad, Pads, Part, Project, Side, Track};
use viamind_drc::{CopperReference, DrcInput, ViolationKind, run_drc};
use viamind_geom::{Nm, Point, Polygon};
fn hole_part(x: i64, diameter: i64) -> Part {
    Part {
        id: ObjectId::new(),
        holes: Some(vec![PartHole { id: ObjectId::new(), key: "mount.1".into(), center: Point::ORIGIN, diameter: Nm(diameter) }]),
        artwork: None,
        assembly: None,
        schematic_symbol: None,
        schematic_package: None,
        designator: "H1".into(),
        value: String::new(),
        footprint: String::new(),
        position: Point::new(Nm(x), Nm(0)),
        rotation_mdeg: 0,
        side: Side::Bottom,
        pads: None,
    }
}
fn track(layer: CopperLayer, net: Option<&str>, y: i64) -> Track {
    Track {
        id: ObjectId::new(),
        start: Point::new(Nm(-2_000_000), Nm(y)),
        end: Point::new(Nm(2_000_000), Nm(y)),
        width: Nm(200_001),
        layer,
        net: net.map(str::to_owned),
    }
}
#[test]
fn mechanical_only_part_drill_constrains_both_layers_and_every_net_without_copper_or_via_minima() {
    for layer in [CopperLayer::Top, CopperLayer::Bottom] {
        for net in [None, Some("N")] {
            let mut p = Project::new("drill");
            let h = hole_part(0, 1_000_001);
            let expected = CopperReference::NonplatedHole { part_id: h.id, hole_id: h.holes.as_ref().unwrap()[0].id, key: "mount.1".into() };
            p.board.parts.insert(h.id, h);
            let t = track(layer, net, 0);
            p.board.tracks.insert(t.id, t);
            let report = run_drc(&p, &DrcInput { minimum_via_drill_mm: 100.0, minimum_annular_width_mm: 100.0, ..Default::default() }).unwrap();
            assert_eq!((report.coverage.checked_pads, report.coverage.checked_vias, report.coverage.checked_holes), (0, 0, 1));
            assert_eq!(report.coverage.excluded_part_count, 1); // Unknown legacy copper remains disclosed.
            assert_eq!(report.violations.len(), 1);
            let issue = &report.violations[0];
            assert_eq!(issue.kind, ViolationKind::HoleToCopperClearance);
            assert!(issue.objects.contains(&expected));
            assert_eq!(expected.selection_id(), p.board.parts.values().next().unwrap().id);
        }
    }
}
#[test]
fn owners_pad_is_not_exempt_and_odd_diameter_gap_uses_existing_float_tolerance() {
    let mut p = Project::new("owner");
    let mut h = hole_part(0, 1_000_001);
    h.pads = Some(
        Pads::try_from(vec![Pad {
            id: ObjectId::new(),
            number: "1".into(),
            position: Point::ORIGIN,
            width: Nm(2_000_000),
            height: Nm(2_000_000),
            net: Some("N".into()),
            apertures: None,
        }])
        .unwrap(),
    );
    p.board.parts.insert(h.id, h);
    let r = run_drc(&p, &Default::default()).unwrap();
    assert_eq!(r.coverage.checked_pads, 1);
    assert_eq!(r.violations.iter().map(|v| v.kind).collect::<Vec<_>>(), [ViolationKind::HoleToCopperClearance]);
    p.board.parts.values_mut().next().unwrap().pads = None;
    let t = track(CopperLayer::Top, Some("N"), 800_001);
    let id = t.id;
    p.board.tracks.insert(id, t);
    assert!(run_drc(&p, &Default::default()).unwrap().violations.is_empty()); // exactly200000Nm radial gap
    p.board.tracks.get_mut(&id).unwrap().start.y -= Nm(1);
    p.board.tracks.get_mut(&id).unwrap().end.y -= Nm(1);
    assert_eq!(run_drc(&p, &Default::default()).unwrap().violations[0].kind, ViolationKind::HoleToCopperClearance);
}
#[test]
fn physical_hole_spacing_and_drill_boundary_are_checked_without_pads() {
    let mut p = Project::new("edge");
    let h = hole_part(0, 1_000_001);
    p.board.parts.insert(h.id, h);
    let h = hole_part(1_250_001, 1_000_001);
    p.board.parts.insert(h.id, h);
    assert!(run_drc(&p, &Default::default()).unwrap().violations.is_empty()); // exact250000Nm gap
    p.board.parts.values_mut().find(|h| h.position.x.0 != 0).unwrap().position.x -= Nm(1);
    assert_eq!(run_drc(&p, &Default::default()).unwrap().violations[0].kind, ViolationKind::HoleClearance);
    p.board.parts.retain(|_, h| h.position.x.0 == 0);
    p.board.outline = Some(
        BoardOutline::new(
            Polygon::new(vec![Point::from_mm(-0.75, -2.0), Point::from_mm(2.0, -2.0), Point::from_mm(2.0, 2.0), Point::from_mm(-0.75, 2.0)]),
            vec![],
        )
        .unwrap(),
    );
    assert_eq!(run_drc(&p, &Default::default()).unwrap().violations[0].kind, ViolationKind::BoardEdgeClearance);
}
#[test]
fn malformed_or_unlocated_holes_refuse_instead_of_silent_exclusion() {
    let mut p = Project::new("bad");
    let mut h = hole_part(0, 1);
    let id = h.id;
    h.position.x = Nm(i64::MAX);
    h.holes.as_mut().unwrap()[0].center.x = Nm(1);
    p.board.parts.insert(id, h);
    assert!(run_drc(&p, &Default::default()).is_err());
    let h = p.board.parts.get_mut(&id).unwrap();
    h.position = Point::ORIGIN;
    h.holes.as_mut().unwrap()[0].diameter = Nm(0);
    assert!(run_drc(&p, &Default::default()).is_err());
}
