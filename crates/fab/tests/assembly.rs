use std::{
    fs,
    sync::{Arc, Barrier},
};

use viamind_doc::{ObjectId, Part, Project, Side};
use viamind_fab::{AssemblyCsv, csv, write_csv};
use viamind_geom::{Nm, Point};

fn sample() -> Project {
    let mut project = Project::new("assembly");
    for (designator, x, rotation, side) in [("R2", i64::MIN, -1, Side::Bottom), ("R1", 1, 360_001, Side::Top)] {
        let part = Part {
            pads: None,
            id: ObjectId::new(),
            designator: designator.into(),
            value: "10k, \"precision\"\r\nµΩ".into(),
            footprint: "0603".into(),
            position: Point::new(Nm(x), Nm(-1_234_567)),
            rotation_mdeg: rotation,
            side,
        };
        project.board.parts.insert(part.id, part);
    }
    project
}

#[test]
fn text_order_exact_units_and_grouping() {
    let project = sample();
    let first = project.board.part_by_designator("R1").unwrap();
    let second = project.board.part_by_designator("R2").unwrap();
    let output = csv(&project, AssemblyCsv::Placement);
    assert!(output.contains(&format!("R1,{},\"10k, \"\"precision\"\"\r\nµΩ\",0603,0.000001,-1.234567,0.001,Top\r\n", first.id)), "{output}");
    assert!(
        output.contains(&format!("R2,{},\"10k, \"\"precision\"\"\r\nµΩ\",0603,-9223372036854.775808,-1.234567,359.999,Bottom\r\n", second.id)),
        "{output}"
    );
    assert!(output.find("R1,").unwrap() < output.find("R2,").unwrap());
    let bom = csv(&project, AssemblyCsv::Bom);
    assert!(bom.contains(&format!("\"[\"\"R1\"\",\"\"R2\"\"]\",{} {},2,", first.id, second.id)), "{bom}");
    assert_eq!(csv(&project, AssemblyCsv::Bom), bom);
    let mut different = project.clone();
    different.board.parts.get_mut(&second.id).unwrap().footprint = "0402".into();
    assert!(csv(&different, AssemblyCsv::Bom).contains(",1,\"10k,"));
    assert_eq!(csv(&Project::new("empty"), AssemblyCsv::Bom), "Designators,Component IDs,Quantity,Value,Footprint\r\n");
}

#[test]
fn publication_refuses_collisions_cleans_temporary_and_protects_native_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("parts.csv");
    let project = sample();
    fs::write(&path, "keep me").unwrap();
    assert!(write_csv(&project, AssemblyCsv::Bom, &path, false, None).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "keep me");
    assert!(write_csv(&project, AssemblyCsv::Bom, &path, true, Some(&path)).is_err());
    let alias = directory.path().join(".").join("parts.csv");
    assert!(write_csv(&project, AssemblyCsv::Bom, &alias, true, Some(&path)).is_err());
    write_csv(&project, AssemblyCsv::Bom, &path, true, None).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), csv(&project, AssemblyCsv::Bom));
    assert!(write_csv(&project, AssemblyCsv::Bom, &directory.path().join("missing/parts.csv"), false, None).is_err());
    assert!(write_csv(&project, AssemblyCsv::Bom, directory.path(), true, None).is_err());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1, "failed publication cleans temporary files");
}

#[test]
fn concurrent_no_clobber_publication_has_one_winner() {
    let directory = tempfile::tempdir().unwrap();
    let path = Arc::new(directory.path().join("race.csv"));
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|index| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut project = sample();
                project.board.parts.values_mut().for_each(|part| part.value = format!("winner{index}"));
                barrier.wait();
                let expected = csv(&project, AssemblyCsv::Bom);
                write_csv(&project, AssemblyCsv::Bom, &path, false, None).map(|_| expected)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|handle| handle.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(fs::read_to_string(path.as_ref()).unwrap(), *results.iter().find_map(|result| result.as_ref().ok()).unwrap());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
