static TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
use std::{
    fs,
    sync::{Arc, Barrier},
};
use viamind_doc::{Project, file};
use viamind_fab::{FabricationOptions, fabrication, write_fabrication};
fn sample() -> Project {
    file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap()
}
#[test]
fn exact_geometry_attributes_units_and_deterministic_output() {
    let _guard = TEST.lock().unwrap();
    let project = sample();
    let package = fabrication(&project, FabricationOptions { mask_expansion_mm: 0.050001, include_silkscreen: false, include_paste: false }).unwrap();
    assert_eq!(
        package.files,
        fabrication(&project, FabricationOptions { mask_expansion_mm: 0.050001, include_silkscreen: false, include_paste: false }).unwrap().files
    );
    assert_eq!(package.manifest.smd_pad_count, 3);
    assert_eq!(package.manifest.cutout_count, 1);
    let bottom = &package.files["bottom-copper.gbr"];
    assert!(bottom.contains("%LR30.000*%"));
    assert!(bottom.contains("%TA.AperFunction,SMDPad,CuDef*%"));
    assert!(bottom.contains("%TF.FileFunction,Copper,L2,Bot*%"));
    assert!(bottom.contains("X30000000Y-17000000D02*\nX40000000Y-9000000D01*"));
    assert!(bottom.contains("C,0.400000*%"));
    assert!(package.files["top-mask.gbr"].contains("%TF.FilePolarity,Negative*%"));
    assert!(package.files["bottom-mask.gbr"].contains("R,4.100002X1.100002*%"));
    let drill = &package.files["plated-through.xnc"];
    assert!(drill.contains("M48\nMETRIC\nT01C0.300000\nT02C0.500000\n%\nG05\n"));
    assert!(drill.contains("X18.000000Y-12.000000"));
    assert_eq!(package.files["profile.gbr"].matches("D01*").count(), 8);
    assert_eq!(package.files.len(), 7);
}
#[test]
fn invalid_or_unsupported_model_produces_no_destination_or_staging() {
    let _guard = TEST.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("fab");
    let mut invalid = sample();
    invalid.board.parts.values_mut().next().unwrap().pads = None;
    assert!(write_fabrication(&invalid, FabricationOptions::default(), &target, None).unwrap_err().to_string().contains("no explicit pads"));
    invalid = sample();
    invalid.board.outline = None;
    assert!(write_fabrication(&invalid, FabricationOptions::default(), &target, None).is_err());
    invalid = sample();
    invalid.board.vias.values_mut().next().unwrap().drill.0 = 0;
    assert!(write_fabrication(&invalid, FabricationOptions::default(), &target, None).is_err());
    for mask in [f64::NAN, f64::INFINITY, -0.1, 10.00001] {
        assert!(
            write_fabrication(
                &sample(),
                FabricationOptions { mask_expansion_mm: mask, include_silkscreen: false, include_paste: false },
                &target,
                None
            )
            .is_err()
        );
    }
    invalid = sample();
    invalid.board.parts.values_mut().next().unwrap().position.x.0 = i64::MAX;
    assert!(write_fabrication(&invalid, FabricationOptions::default(), &target, None).is_err());
    invalid = sample();
    invalid.board.outline.as_mut().unwrap().outer.points.clear();
    assert!(write_fabrication(&invalid, FabricationOptions::default(), &target, None).is_err());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}
#[test]
fn no_replace_directory_publication_guards_native_symlinks_and_races() {
    let _guard = TEST.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("fab");
    fs::create_dir(&target).unwrap();
    assert!(write_fabrication(&sample(), FabricationOptions::default(), &target, None).is_err());
    assert!(fs::read_dir(&target).unwrap().next().is_none());
    fs::remove_dir(&target).unwrap();
    fs::write(&target, "native").unwrap();
    assert!(write_fabrication(&sample(), FabricationOptions::default(), &target, Some(&target)).unwrap_err().to_string().contains("active native"));
    assert_eq!(fs::read_to_string(&target).unwrap(), "native");
    #[cfg(unix)]
    {
        let link = dir.path().join("alias");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(write_fabrication(&sample(), FabricationOptions::default(), &link, Some(&target)).is_err());
        fs::remove_file(link).unwrap();
    }
    fs::remove_file(&target).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path().join("missing"), &target).unwrap();
        assert!(write_fabrication(&sample(), FabricationOptions::default(), &target, None).is_err());
        assert!(target.is_symlink());
        fs::remove_file(&target).unwrap();
    }
    let barrier = Arc::new(Barrier::new(2));
    let jobs: Vec<_> = (0..2)
        .map(|_| {
            let barrier = barrier.clone();
            let target = target.clone();
            std::thread::spawn(move || {
                barrier.wait();
                write_fabrication(&sample(), FabricationOptions::default(), &target, None)
            })
        })
        .collect();
    assert_eq!(jobs.into_iter().filter_map(|j| j.join().unwrap().ok()).count(), 1);
    let expected = fabrication(&sample(), FabricationOptions::default()).unwrap();
    for (name, text) in expected.files {
        assert_eq!(fs::read_to_string(target.join(name)).unwrap(), text);
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn export_primitive_bound_rejects_before_any_files_are_created() {
    let _guard = TEST.lock().unwrap();
    let mut project = sample();
    let template = project.board.vias.values().next().unwrap().clone();
    for _ in 0..40000 {
        let mut via = template.clone();
        via.id = viamind_doc::ObjectId::new();
        project.board.vias.insert(via.id, via);
    }
    let dir = tempfile::tempdir().unwrap();
    let error = write_fabrication(&project, FabricationOptions::default(), &dir.path().join("large"), None).unwrap_err();
    assert!(error.to_string().contains("200000 primitives"));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn empty_geometry_and_xnc_two_digit_tool_boundary_are_explicit() {
    let _guard = TEST.lock().unwrap();
    let mut project = sample();
    project.board.parts.clear();
    project.board.tracks.clear();
    project.board.vias.clear();
    let empty = fabrication(&project, FabricationOptions::default()).unwrap();
    assert_eq!(empty.manifest.smd_pad_count, 0);
    assert_eq!(empty.manifest.through_via_count, 0);
    assert!(empty.files["plated-through.xnc"].ends_with("%\nG05\nM30\n"));
    let template = sample().board.vias.values().next().unwrap().clone();
    for i in 1..=99 {
        let mut via = template.clone();
        via.id = viamind_doc::ObjectId::new();
        via.drill.0 = i * 1000;
        project.board.vias.insert(via.id, via);
    }
    let maximum = fabrication(&project, FabricationOptions::default()).unwrap();
    assert!(maximum.files["plated-through.xnc"].contains("T99C0.099000"));
    let mut via = template;
    via.id = viamind_doc::ObjectId::new();
    via.drill.0 = 100_000;
    project.board.vias.insert(via.id, via);
    let directory = tempfile::tempdir().unwrap();
    let error = write_fabrication(&project, FabricationOptions::default(), &directory.path().join("too-many-tools"), None).unwrap_err();
    assert!(error.to_string().contains("99 distinct"));
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}
