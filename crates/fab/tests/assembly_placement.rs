use std::sync::Mutex;
use viamind_doc::{AssemblyMetadata, AssemblyPlacement, MountingTechnology, ObjectId, Part, Project, Side};
use viamind_fab::{AssemblyCsv, AssemblySnapshot, csv};
use viamind_geom::{Nm, Point};
static TEST: Mutex<()> = Mutex::new(());
fn sample() -> Project {
    let mut project = Project::new("calibration");
    for (name, side) in [("U1", Side::Top), ("U2", Side::Bottom)] {
        let part = Part {
            id: ObjectId::new(),
            designator: name.into(),
            value: "µ, \"part\"\r\nvalue".into(),
            footprint: "asymmetric\nfootprint".into(),
            position: Point::from_mm(10., 20.),
            rotation_mdeg: 90_000,
            side,
            pads: None,
            schematic_symbol: None,
            assembly: Some(Box::new(AssemblyMetadata {
                mounting: Some(MountingTechnology::SurfaceMount),
                placement: Some(AssemblyPlacement { centroid_local: Point::from_mm(2., 1.), zero_axis_mdeg: 37_125 }),
                ..Default::default()
            })),
        };
        project.board.parts.insert(part.id, part);
    }
    project
}
fn parsed(project: &Project) -> serde_json::Value {
    let text = csv(project, AssemblyCsv::AssemblyPlacement).unwrap();
    let mut file = tempfile::NamedTempFile::new().unwrap();
    use std::io::Write;
    file.write_all(text.as_bytes()).unwrap();
    let result = std::process::Command::new("python3")
        .args(["-c", "import csv,json,sys;print(json.dumps(list(csv.DictReader(open(sys.argv[1],encoding='utf-8',newline='')))))"])
        .arg(file.path())
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    serde_json::from_slice(&result.stdout).unwrap()
}
#[test]
fn asymmetric_centroids_mirrored_markers_and_escaped_text_parse_independently() {
    let _guard = TEST.lock().unwrap();
    let project = sample();
    let rows = parsed(&project);
    assert_eq!(rows.as_array().unwrap().len(), 2);
    for (index, side, y, rotation) in [(0, "top", "-18.000000", "127.125"), (1, "bottom", "-22.000000", "232.875")] {
        assert_eq!(rows[index]["X (mm, global)"], "11.000000");
        assert_eq!(rows[index]["Y (mm, up)"], y);
        assert_eq!(rows[index]["Rotation (deg CCW, top view)"], rotation);
        assert_eq!(rows[index]["Side"], side);
        assert_eq!(rows[index]["Value"], "µ, \"part\"\r\nvalue");
        assert_eq!(rows[index]["Footprint"], "asymmetric\nfootprint");
        assert_eq!(rows[index]["Mounting"], "surface_mount");
    }
    assert_eq!(rows[0]["Designator"], "U1");
    assert_eq!(rows[1]["Designator"], "U2");
    assert_eq!(csv(&project, AssemblyCsv::AssemblyPlacement).unwrap(), csv(&project, AssemblyCsv::AssemblyPlacement).unwrap());
}
#[test]
fn noncardinal_reflection_matches_independent_vector_oracle_and_output_is_bounded() {
    let _guard = TEST.lock().unwrap();
    let mut project = sample();
    for part in project.board.parts.values_mut() {
        part.rotation_mdeg = 33_700;
    }
    let rows = parsed(&project);
    let r = 33.7_f64.to_radians();
    let marker = 37.125_f64.to_radians();
    for (index, sign) in [(0, 1.), (1, -1.)] {
        let x = 10. + sign * 2. * r.cos() + r.sin();
        let y = -(20. - sign * 2. * r.sin() + r.cos());
        let axis_x = sign * marker.cos() * r.cos() - marker.sin() * r.sin();
        let axis_y = sign * marker.cos() * r.sin() + marker.sin() * r.cos();
        let rotation = axis_y.atan2(axis_x).to_degrees().rem_euclid(360.);
        let number = |name| rows[index][name].as_str().unwrap().parse::<f64>().unwrap();
        assert!((number("X (mm, global)") - x).abs() <= 0.000001);
        assert!((number("Y (mm, up)") - y).abs() <= 0.000001);
        assert!((number("Rotation (deg CCW, top view)") - rotation).abs() < 0.000001);
    }
    let first = project.board.parts.keys().next().copied().unwrap();
    for position in [Nm(i64::MIN), Nm(i64::MAX), Nm(99_999_999_999)] {
        project.board.parts.get_mut(&first).unwrap().position = Point::new(position, position);
        assert!(csv(&project, AssemblyCsv::AssemblyPlacement).is_err());
    }
    project.board.parts.get_mut(&first).unwrap().position = Point::ORIGIN;
    project.board.parts.get_mut(&first).unwrap().rotation_mdeg = -1;
    assert!(csv(&project, AssemblyCsv::AssemblyPlacement).is_err());
}
#[test]
fn population_completeness_reference_collisions_and_purpose_specific_captures() {
    let _guard = TEST.lock().unwrap();
    let mut project = sample();
    let first = project.board.part_by_designator("U1").unwrap().id;
    let second = project.board.part_by_designator("U2").unwrap().id;
    let expected = csv(&project, AssemblyCsv::AssemblyPlacement).unwrap();
    for part in project.board.parts.values_mut() {
        part.assembly.as_mut().unwrap().mpn = Some("\n".repeat(3 * 1024 * 1024));
    }
    assert_eq!(csv(&project, AssemblyCsv::AssemblyPlacement).unwrap(), expected, "unused sourcing is excluded before copy/validation");
    assert!(csv(&project, AssemblyCsv::SourcingBom).is_err());
    assert!(AssemblySnapshot::capture(&project).unwrap().csv(AssemblyCsv::AssemblyPlacement).is_err());
    let snapshot = AssemblySnapshot::capture_for(&project, AssemblyCsv::AssemblyPlacement).unwrap();
    assert!(snapshot.csv(AssemblyCsv::SourcingBom).is_err());
    drop(snapshot);
    let part = project.board.parts.get_mut(&second).unwrap();
    part.designator = "bad ref".into();
    part.assembly.as_mut().unwrap().populate = false;
    part.assembly.as_mut().unwrap().placement = None;
    let snapshot = AssemblySnapshot::capture_for(&project, AssemblyCsv::AssemblyPlacement).unwrap();
    assert_eq!(snapshot.component_count_for(AssemblyCsv::AssemblyPlacement), 1);
    drop(snapshot);
    for incomplete in
        [None, Some(AssemblyMetadata::default()), Some(AssemblyMetadata { mounting: Some(MountingTechnology::Hybrid), ..Default::default() })]
    {
        project.board.parts.get_mut(&first).unwrap().assembly = incomplete.map(Box::new);
        assert!(csv(&project, AssemblyCsv::AssemblyPlacement).is_err());
    }
    project = sample();
    let second = project.board.part_by_designator("U2").unwrap().id;
    project.board.parts.get_mut(&second).unwrap().designator = "u1".into();
    assert!(csv(&project, AssemblyCsv::AssemblyPlacement).is_err());
    for part in project.board.parts.values_mut() {
        part.assembly.as_mut().unwrap().populate = false;
    }
    assert_eq!(csv(&project, AssemblyCsv::AssemblyPlacement).unwrap(), csv(&Project::new("empty"), AssemblyCsv::AssemblyPlacement).unwrap());
}
#[test]
fn older_csv_formats_ignore_unused_calibration_and_keep_exact_bytes() {
    let _guard = TEST.lock().unwrap();
    let mut project = viamind_doc::file::from_str(include_str!("../../../fixtures/assembly/bom-templates.viamind.json")).unwrap();
    let kinds = [AssemblyCsv::Bom, AssemblyCsv::Placement, AssemblyCsv::SourcingBom, AssemblyCsv::CatalogBom, AssemblyCsv::AssemblyBom];
    let before = kinds.map(|kind| csv(&project, kind).unwrap());
    for part in project.board.parts.values_mut() {
        let metadata = part.assembly.get_or_insert_with(Default::default);
        metadata.mounting = Some(MountingTechnology::ThroughHole);
        metadata.placement = Some(AssemblyPlacement { centroid_local: Point::new(Nm(i64::MAX), Nm(i64::MIN)), zero_axis_mdeg: -1 });
    }
    for (kind, expected) in kinds.into_iter().zip(before) {
        assert_eq!(csv(&project, kind).unwrap(), expected);
    }
    assert!(csv(&project, AssemblyCsv::AssemblyPlacement).is_err());
}
