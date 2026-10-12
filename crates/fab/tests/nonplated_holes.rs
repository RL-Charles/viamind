//! Explicit independent coordinate/tool/file oracles; existing outputs stay byte-exact.
use viamind_doc::{ObjectId, Side, file, footprint_holes::PartHole};
use viamind_fab::{FabricationOptions, OutputJob, OutputJobSnapshot, fabrication, write_fabrication};
use viamind_geom::{Nm, Point};
fn sample() -> viamind_doc::Project {
    file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap()
}
#[test]
fn opt_in_refuses_even_explicit_empty_and_new_output_has_valid_empty_drill_file() {
    let mut p = sample();
    let legacy = fabrication(&p, Default::default()).unwrap();
    let enabled = fabrication(&p, FabricationOptions { include_npth: true, ..Default::default() }).unwrap();
    for (name, bytes) in &legacy.files {
        if name != "manifest.json" {
            assert_eq!(&enabled.files[name], bytes);
        }
    }
    assert_eq!(enabled.files.len(), legacy.files.len() + 1);
    assert!(enabled.files["nonplated-through.xnc"].ends_with("M48\nMETRIC\n%\nG05\nM30\n"));
    assert!(legacy.manifest.nonplated_holes.is_none());
    assert_eq!(enabled.manifest.nonplated_holes.unwrap().physical_drills, 0);
    p.board.parts.values_mut().next().unwrap().holes = Some(vec![]);
    assert!(fabrication(&p, Default::default()).err().unwrap().to_string().contains("include_npth"));
    let out = fabrication(&p, FabricationOptions { include_npth: true, ..Default::default() }).unwrap();
    let summary = out.manifest.nonplated_holes.unwrap();
    assert_eq!((summary.authored_components, summary.explicit_empty_components, summary.physical_drills, summary.tool_diameters), (1, 1, 0, 0));
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("never");
    assert!(write_fabrication(&p, Default::default(), &destination, None).is_err());
    assert!(!destination.exists());
    assert_eq!(dir.path().read_dir().unwrap().count(), 0);
    assert!(OutputJobSnapshot::capture(&p, OutputJob::new("old".into(), 0.0)).err().unwrap().to_string().contains("job6"));
}
#[test]
fn exact_eight_pose_npth_uses_diameter_then_owner_id_order_and_single_y_flip() {
    let mut p = sample();
    let old = fabrication(&p, Default::default()).unwrap();
    let seed = p.board.parts.values().next().unwrap().clone();
    for (i, (side, angle, diameter)) in [
        (Side::Top, 0, 7),
        (Side::Top, 90000, 3),
        (Side::Top, 180000, 5),
        (Side::Top, 270000, 7),
        (Side::Bottom, 0, 3),
        (Side::Bottom, 90000, 5),
        (Side::Bottom, 180000, 7),
        (Side::Bottom, 270000, 3),
    ]
    .into_iter()
    .enumerate()
    {
        let mut part = seed.clone();
        part.id = format!("00000000-0000-0000-0000-{:012x}", 100 + i).parse().unwrap();
        part.designator = format!("H{i}");
        part.side = side;
        part.rotation_mdeg = angle;
        part.pads = Some(viamind_doc::Pads(vec![]));
        part.artwork = None;
        part.position = Point::new(Nm(10_000_000), Nm(20_000_000));
        part.holes = Some(vec![PartHole {
            id: format!("00000000-0000-0000-0000-{:012x}", 200 + i).parse().unwrap(),
            key: format!("mount{i}"),
            center: Point::new(Nm(-3), Nm(5)),
            diameter: Nm(diameter),
        }]);
        p.board.parts.insert(part.id, part);
    }
    let out = fabrication(&p, FabricationOptions { include_npth: true, ..Default::default() }).unwrap();
    for (name, bytes) in &old.files {
        if name != "manifest.json" {
            assert_eq!(&out.files[name], bytes, "existing file{name}");
        }
    }
    let text = &out.files["nonplated-through.xnc"];
    assert!(text.contains("; #@! TF.FileFunction,NonPlated,1,2,NPTH\n"));
    assert!(text.contains(&format!("; #@! TF.SameCoordinates,{}\n", p.id)));
    let expected = "M48\nMETRIC\nT01C0.000003\nT02C0.000005\nT03C0.000007\n%\nG05\nT01\nX10.000005Y-20.000003\nX10.000003Y-20.000005\nX9.999995Y-20.000003\nT02\nX10.000003Y-19.999995\nX10.000005Y-19.999997\nT03\nX9.999997Y-20.000005\nX9.999995Y-19.999997\nX9.999997Y-19.999995\nM30\n";
    assert_eq!(&text[text.find("M48\n").unwrap()..], expected);
    let (geometry, tools) = viamind_doc::fabrication_view::parse_nonplated_xnc(text, &mut Default::default()).unwrap();
    assert_eq!(geometry.layer, viamind_doc::fabrication_view::FabricationLayer::NonplatedDrill);
    assert_eq!(geometry.same_coordinates, Some(p.id));
    assert!(!geometry.unverified_common_origin);
    assert_eq!(tools, 3);
    let observed: Vec<_> = geometry
        .primitives
        .iter()
        .map(|p| match p {
            viamind_doc::fabrication_view::FabricationPrimitive::Circle { center, diameter } => (center.x.0, center.y.0, diameter.0),
            _ => panic!("drill circle"),
        })
        .collect();
    assert_eq!(
        observed,
        [
            (10000005, 20000003, 3),
            (10000003, 20000005, 3),
            (9999995, 20000003, 3),
            (10000003, 19999995, 5),
            (10000005, 19999997, 5),
            (9999997, 20000005, 7),
            (9999995, 19999997, 7),
            (9999997, 19999995, 7)
        ]
    );
    let s = out.manifest.nonplated_holes.unwrap();
    assert_eq!((s.authored_components, s.physical_drills, s.tool_diameters), (8, 8, 3));
}
#[test]
fn tool_max_plus_one_refuses_without_publication_and_does_not_restrict_plated_tool_file() {
    let mut p = sample();
    let part = p.board.parts.values_mut().next().unwrap();
    part.holes = Some(
        (1..=99).map(|i| PartHole { id: ObjectId::new(), key: format!("H{i}"), center: Point::new(Nm(i * 100), Nm(0)), diameter: Nm(i) }).collect(),
    );
    let out = fabrication(&p, FabricationOptions { include_npth: true, ..Default::default() }).unwrap();
    assert_eq!(out.manifest.nonplated_holes.unwrap().tool_diameters, 99);
    assert!(out.files["nonplated-through.xnc"].contains("T99C0.000099\n"));
    p.board.parts.values_mut().next().unwrap().holes.as_mut().unwrap().push(PartHole {
        id: ObjectId::new(),
        key: "H100".into(),
        center: Point::ORIGIN,
        diameter: Nm(100),
    });
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("never");
    let error = write_fabrication(&p, FabricationOptions { include_npth: true, ..Default::default() }, &destination, None).unwrap_err();
    assert!(error.to_string().contains("99 distinct nonplated"));
    assert!(!destination.exists());
    assert_eq!(dir.path().read_dir().unwrap().count(), 0);
}
