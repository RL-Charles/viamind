use std::sync::Mutex;
use viamind_doc::{ObjectId, Project, Sheet, file};
use viamind_fab::{AssemblyCsv, AssemblySnapshot, FabricationOptions, FabricationSnapshot, csv, fabrication, write_csv, write_fabrication};
static TEST: Mutex<()> = Mutex::new(());
fn sample() -> Project {
    file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap()
}

#[test]
fn unrelated_schematic_and_assembly_geometry_are_excluded_from_direct_calls() {
    let _guard = TEST.lock().unwrap();
    let mut project = sample();
    let package = fabrication(&project, FabricationOptions::default()).unwrap();
    let bom = csv(&project, AssemblyCsv::Bom).unwrap();
    let placement = csv(&project, AssemblyCsv::Placement).unwrap();
    // Invalid unrelated sheet identity would collide with a pad in whole-project validators.
    let id = project.board.parts.values().next().unwrap().pads.as_ref().unwrap().0[0].id;
    let mut sheet = Sheet::new("unrelated".repeat(500_000));
    sheet.id = ObjectId::new();
    project.schematic.sheets.insert(id, sheet);
    assert_eq!(fabrication(&project, FabricationOptions::default()).unwrap().files, package.files);
    // Assembly does not traverse malformed/large pad metadata, copper or outlines.
    project.board.parts.values_mut().next().unwrap().pads.as_mut().unwrap().0[0].number = "x".repeat(3 * 1024 * 1024);
    project.name = "unrelated project title".repeat(200_000);
    assert_eq!(csv(&project, AssemblyCsv::Bom).unwrap(), bom);
    assert_eq!(csv(&project, AssemblyCsv::Placement).unwrap(), placement);
    assert!(FabricationSnapshot::capture(&project).is_err());
}

#[test]
fn text_and_component_budgets_reject_before_publication_and_preserve_existing_bytes() {
    let _guard = TEST.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keep.csv");
    std::fs::write(&path, "keep").unwrap();
    let mut project = sample();
    project.board.parts.values_mut().next().unwrap().value = "µ".repeat(2048);
    assert!(csv(&project, AssemblyCsv::Bom).is_ok());
    project.board.parts.values_mut().next().unwrap().value.push('x');
    assert!(write_csv(&project, AssemblyCsv::Bom, &path, true, None).unwrap_err().to_string().contains("4096"));
    assert!(write_fabrication(&project, FabricationOptions::default(), &dir.path().join("fab"), None).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
    let template = sample().board.parts.into_values().next().unwrap();
    project.board.parts.clear();
    for _ in 0..513 {
        let mut part = template.clone();
        part.id = ObjectId::new();
        part.value = "x".repeat(4096);
        project.board.parts.insert(part.id, part);
    }
    assert!(AssemblySnapshot::capture(&project).err().unwrap().to_string().contains("aggregate"));
    assert!(FabricationSnapshot::capture(&project).err().unwrap().to_string().contains("aggregate"));
    project.board.parts.clear();
    let mut part = template;
    part.pads = None;
    part.designator.clear();
    part.value.clear();
    part.footprint.clear();
    for _ in 0..100_001 {
        part.id = ObjectId::new();
        project.board.parts.insert(part.id, part.clone());
    }
    assert!(AssemblySnapshot::capture(&project).err().unwrap().to_string().contains("100000"));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn admission_is_shared_and_releases_after_failed_capture_or_drop() {
    let _guard = TEST.lock().unwrap();
    let project = sample();
    let assembly = AssemblySnapshot::capture(&project).unwrap();
    let fab = FabricationSnapshot::capture(&project).unwrap();
    assert_eq!(AssemblySnapshot::capture(&project).err().unwrap().kind(), std::io::ErrorKind::WouldBlock);
    assert_eq!(FabricationSnapshot::capture(&project).err().unwrap().kind(), std::io::ErrorKind::WouldBlock);
    drop(assembly);
    let mut bad = project.clone();
    bad.name = "x".repeat(4097);
    assert!(FabricationSnapshot::capture(&bad).is_err());
    let replacement = AssemblySnapshot::capture(&project).unwrap();
    assert_eq!(replacement.component_count(), project.board.parts.len());
    assert_eq!(replacement.csv(AssemblyCsv::Bom).unwrap(), replacement.csv(AssemblyCsv::Bom).unwrap());
    drop((replacement, fab));
    assert!(csv(&project, AssemblyCsv::Bom).is_ok());
}
