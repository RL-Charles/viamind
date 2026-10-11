static TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
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
            assembly: None,
            schematic_symbol: None,
            schematic_package: None,
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
    let _guard = TEST.lock().unwrap();
    let project = sample();
    let first = project.board.part_by_designator("R1").unwrap();
    let second = project.board.part_by_designator("R2").unwrap();
    let output = csv(&project, AssemblyCsv::Placement).unwrap();
    assert!(output.contains(&format!("R1,{},\"10k, \"\"precision\"\"\r\nµΩ\",0603,0.000001,-1.234567,0.001,Top\r\n", first.id)), "{output}");
    assert!(
        output.contains(&format!("R2,{},\"10k, \"\"precision\"\"\r\nµΩ\",0603,-9223372036854.775808,-1.234567,359.999,Bottom\r\n", second.id)),
        "{output}"
    );
    assert!(output.find("R1,").unwrap() < output.find("R2,").unwrap());
    let bom = csv(&project, AssemblyCsv::Bom).unwrap();
    assert!(bom.contains(&format!("\"[\"\"R1\"\",\"\"R2\"\"]\",{} {},2,", first.id, second.id)), "{bom}");
    assert_eq!(csv(&project, AssemblyCsv::Bom).unwrap(), bom);
    let mut different = project.clone();
    different.board.parts.get_mut(&second.id).unwrap().footprint = "0402".into();
    assert!(csv(&different, AssemblyCsv::Bom).unwrap().contains(",1,\"10k,"));
    assert_eq!(csv(&Project::new("empty"), AssemblyCsv::Bom).unwrap(), "Designators,Component IDs,Quantity,Value,Footprint\r\n");
}

#[test]
fn sourcing_groups_exact_metadata_and_population_without_changing_existing_formats() {
    let _guard = TEST.lock().unwrap();
    let mut project = sample();
    let bom = csv(&project, AssemblyCsv::Bom).unwrap();
    let placement = csv(&project, AssemblyCsv::Placement).unwrap();
    let first = project.board.part_by_designator("R1").unwrap().id;
    let second = project.board.part_by_designator("R2").unwrap().id;
    let metadata = viamind_doc::AssemblyMetadata { manufacturer: Some("µ, \"Demo\"".into()), mpn: Some("USER-MPN".into()), ..Default::default() };
    project.board.parts.get_mut(&first).unwrap().assembly = Some(Box::new(metadata.clone()));
    project.board.parts.get_mut(&second).unwrap().assembly = Some(Box::new(metadata));
    let grouped = csv(&project, AssemblyCsv::SourcingBom).unwrap();
    assert!(grouped.contains(",2,\"10k,"));
    assert!(grouped.contains("\"µ, \"\"Demo\"\"\",USER-MPN,,,Populate\r\n"));
    project.board.parts.get_mut(&second).unwrap().assembly.as_mut().unwrap().populate = false;
    let separate = csv(&project, AssemblyCsv::SourcingBom).unwrap();
    assert!(separate.contains("Do not populate\r\n"));
    assert_eq!(separate.matches(",1,\"10k,").count(), 2);
    assert_eq!(csv(&project, AssemblyCsv::Bom).unwrap(), bom);
    assert_eq!(csv(&project, AssemblyCsv::Placement).unwrap(), placement);
    assert!(viamind_fab::AssemblySnapshot::capture(&project).unwrap().csv(AssemblyCsv::SourcingBom).is_err());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sourcing.csv");
    write_csv(&project, AssemblyCsv::SourcingBom, &path, false, None).unwrap();
    let parsed = std::process::Command::new("python3")
        .args(["-c", "import csv,json,sys;print(json.dumps(list(csv.DictReader(open(sys.argv[1],newline='')))))"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(parsed.status.success());
    let rows: serde_json::Value = serde_json::from_slice(&parsed.stdout).unwrap();
    assert_eq!(rows[0]["Manufacturer"], "µ, \"Demo\"");
    assert_eq!(rows[0]["Value"], "10k, \"precision\"\r\nµΩ");
    assert_eq!(rows[0]["Population"], "Do not populate");
    assert_eq!(rows[1]["Population"], "Populate");
    // Unused invalid sourcing text is not cloned/validated by older generic captures.
    project.board.parts.get_mut(&first).unwrap().assembly.as_mut().unwrap().mpn = Some("x".repeat(2 * 1024 * 1024 + 1));
    assert_eq!(csv(&project, AssemblyCsv::Bom).unwrap(), bom);
    assert_eq!(csv(&project, AssemblyCsv::Placement).unwrap(), placement);
    assert!(csv(&project, AssemblyCsv::SourcingBom).is_err());
}

#[test]
fn fabrication_and_saved_jobs_exclude_unused_sourcing_before_copy() {
    let _guard = TEST.lock().unwrap();
    let mut project = viamind_doc::file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let expected = viamind_fab::fabrication(&project, Default::default()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let job =
        viamind_fab::OutputJob::parse(br#"{"format":"viamind-output-job","version":1,"name":"Test","profile":"generic_outer_2layer"}"#).unwrap();
    viamind_fab::OutputJobSnapshot::capture(&project, job.clone()).unwrap().write(0, &dir.path().join("before"), None).unwrap();
    for part in project.board.parts.values_mut() {
        part.assembly =
            Some(Box::new(viamind_doc::AssemblyMetadata { mpn: Some("unrelated".repeat(300000)), populate: false, ..Default::default() }));
    }
    assert_eq!(viamind_fab::FabricationSnapshot::capture(&project).unwrap().render(Default::default()).unwrap().files, expected.files);
    viamind_fab::OutputJobSnapshot::capture(&project, job).unwrap().write(0, &dir.path().join("after"), None).unwrap();
    let compare=std::process::Command::new("python3").args(["-c","import pathlib,sys; a,b=map(pathlib.Path,sys.argv[1:]); files=sorted(p.relative_to(a) for p in a.rglob('*') if p.is_file()); assert len(files)==11; assert all((a/p).read_bytes()==(b/p).read_bytes() for p in files)"]).arg(dir.path().join("before")).arg(dir.path().join("after")).output().unwrap();
    assert!(compare.status.success(), "{}", String::from_utf8_lossy(&compare.stderr));
}

#[test]
fn publication_refuses_collisions_cleans_temporary_and_protects_native_path() {
    let _guard = TEST.lock().unwrap();
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
    assert_eq!(fs::read_to_string(&path).unwrap(), csv(&project, AssemblyCsv::Bom).unwrap());
    assert!(write_csv(&project, AssemblyCsv::Bom, &directory.path().join("missing/parts.csv"), false, None).is_err());
    assert!(write_csv(&project, AssemblyCsv::Bom, directory.path(), true, None).is_err());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1, "failed publication cleans temporary files");
}

#[test]
fn concurrent_no_clobber_publication_has_one_winner() {
    let _guard = TEST.lock().unwrap();
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
                let expected = csv(&project, AssemblyCsv::Bom).unwrap();
                write_csv(&project, AssemblyCsv::Bom, &path, false, None).map(|_| expected)
            })
        })
        .collect();
    let results: Vec<_> = handles.into_iter().map(|handle| handle.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(fs::read_to_string(path.as_ref()).unwrap(), *results.iter().find_map(|result| result.as_ref().ok()).unwrap());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
