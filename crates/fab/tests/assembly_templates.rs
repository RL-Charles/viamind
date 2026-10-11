use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};
use viamind_doc::{ObjectId, Project, file};
use viamind_fab::{AssemblyCsv, AssemblySnapshot, OutputJob, csv, write_csv};
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn fixture() -> Project {
    file::from_str(include_str!("../../../fixtures/assembly/bom-templates.viamind.json")).unwrap()
}
fn rows(path: &Path) -> Value {
    let out = Command::new("python3")
        .args(["-c", "import csv,json,sys; print(json.dumps(list(csv.reader(open(sys.argv[1],encoding='utf-8',newline='')))))"])
        .arg(path)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn exact_templates_unicode_population_and_old_outputs_are_preserved() {
    let _serial = SERIAL.lock().unwrap();
    let project = fixture();
    let before = serde_json::to_vec(&project).unwrap();
    let inventory: Vec<_> =
        [AssemblyCsv::Bom, AssemblyCsv::Placement, AssemblyCsv::SourcingBom].into_iter().map(|kind| (kind, csv(&project, kind).unwrap())).collect();
    let fabrication = viamind_fab::fabrication(&project, Default::default()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    for kind in [AssemblyCsv::CatalogBom, AssemblyCsv::AssemblyBom] {
        let snapshot = AssemblySnapshot::capture_for(&project, kind).unwrap();
        assert_eq!(snapshot.component_count(), 2);
        assert_eq!(snapshot.component_count_for(kind), 1);
        let path = temp.path().join(format!("{}.csv", kind.name()));
        snapshot.write(kind, &path, false, None).unwrap();
        let parsed = rows(&path);
        assert_eq!(parsed.as_array().unwrap().len(), 2);
        if kind == AssemblyCsv::CatalogBom {
            assert_eq!(parsed[0], json!(["Comment", "Designator", "Footprint", "JLCPCB Part #（optional）"]));
            assert_eq!(parsed[1], json!(["10k, \"μΩ\"", "U_TOP", "0603", "C0000000000000000"]));
        } else {
            assert_eq!(
                parsed[0],
                json!([
                    "Item #",
                    "*Designator",
                    "*Qty",
                    "Manufacturer",
                    "*Mfg Part #",
                    "Description / Value",
                    "*Package/Footprint ",
                    "Type",
                    "Your Instructions / Notes"
                ])
            );
            assert_eq!(parsed[1], json!(["1", "U_TOP", "1", "User μ, \"Example\"", "USER-MPN, \"μ\"", "10k, \"μΩ\"", "0603", "", ""]));
        }
        assert!(std::str::from_utf8(&fs::read(path).unwrap()).unwrap().ends_with("\r\n"));
        assert!(AssemblySnapshot::capture(&project).unwrap().csv(kind).is_err());
    }
    for (kind, bytes) in inventory {
        assert_eq!(csv(&project, kind).unwrap(), bytes);
    }
    assert_eq!(viamind_fab::fabrication(&project, Default::default()).unwrap().files, fabrication.files);
    assert_eq!(serde_json::to_vec(&project).unwrap(), before);
    for kind in ["catalog_bom", "assembly_bom"] {
        assert!(OutputJob::parse(format!(r#"{{"format":"viamind-output-job","version":1,"name":"reject","profile":"{kind}"}}"#).as_bytes()).is_err());
        assert!(
            OutputJob::parse(
                format!(r#"{{"format":"viamind-output-job","version":1,"name":"reject","profile":"generic_outer_2layer","kind":"{kind}"}}"#)
                    .as_bytes()
            )
            .is_err()
        );
    }
}
#[test]
fn two_hundred_reference_split_quantity_order_and_distinct_supplier_records() {
    let _serial = SERIAL.lock().unwrap();
    let mut project = fixture();
    let template = project.board.parts.values().find(|p| p.assembly.as_ref().unwrap().populate).unwrap().clone();
    project.board.parts.clear();
    for index in (0..201).rev() {
        let mut part = template.clone();
        part.id = ObjectId::new();
        part.designator = format!("R{index:03}");
        project.board.parts.insert(part.id, part);
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("split.csv");
    write_csv(&project, AssemblyCsv::AssemblyBom, &path, false, None).unwrap();
    let parsed = rows(&path);
    assert_eq!(parsed.as_array().unwrap().len(), 3);
    assert_eq!(parsed[1][0], "1");
    assert_eq!(parsed[2][0], "2");
    assert_eq!(parsed[1][2], "200");
    assert_eq!(parsed[2][2], "1");
    let references: Vec<_> = parsed[1][1].as_str().unwrap().split(',').collect();
    assert_eq!(references.len(), 200);
    assert_eq!(references[0], "R000");
    assert_eq!(references[199], "R199");
    assert_eq!(parsed[2][1], "R200");
    write_csv(&project, AssemblyCsv::CatalogBom, &path, true, None).unwrap();
    let parsed = rows(&path);
    assert_eq!(parsed.as_array().unwrap().len(), 3);
    assert_eq!(parsed[2][1], "R200");
    project.board.parts.values_mut().next().unwrap().assembly.as_mut().unwrap().supplier = Some("JLCPCB".into());
    write_csv(&project, AssemblyCsv::AssemblyBom, &path, true, None).unwrap();
    let parsed = rows(&path);
    assert_eq!(parsed.as_array().unwrap().len(), 3);
    assert_eq!(parsed[1][2], "1");
    assert_eq!(parsed[2][2], "200");
}
#[test]
fn required_fields_case_collisions_and_sku_errors_fail_before_publication() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("keep.csv");
    fs::write(&path, b"keep").unwrap();
    for kind in [AssemblyCsv::CatalogBom, AssemblyCsv::AssemblyBom] {
        for bad in ["", "1R", "R,1", "R 1", "R\n1", "Ré1"] {
            let mut p = fixture();
            p.board.parts.values_mut().find(|p| p.assembly.as_ref().unwrap().populate).unwrap().designator = bad.into();
            assert!(write_csv(&p, kind, &path, true, None).unwrap_err().to_string().contains("designator"));
        }
        let mut p = fixture();
        let first = p.board.parts.values().find(|p| p.assembly.as_ref().unwrap().populate).unwrap().clone();
        let mut duplicate = first.clone();
        duplicate.id = ObjectId::new();
        duplicate.designator = first.designator.to_ascii_lowercase();
        p.board.parts.insert(duplicate.id, duplicate);
        assert!(csv(&p, kind).unwrap_err().to_string().contains("case folding"));
        for bad in ["", " 0603", "0603\n"] {
            let mut p = fixture();
            p.board.parts.values_mut().find(|p| p.assembly.as_ref().unwrap().populate).unwrap().footprint = bad.into();
            assert!(csv(&p, kind).is_err());
        }
    }
    for (field, bad) in [
        ("supplier", "Other"),
        ("supplier", ""),
        ("supplier_sku", "c123"),
        ("supplier_sku", "C"),
        ("supplier_sku", "C1,2"),
        ("supplier_sku", "C１２"),
    ] {
        let mut p = fixture();
        let m = p.board.parts.values_mut().find(|p| p.assembly.as_ref().unwrap().populate).unwrap().assembly.as_mut().unwrap();
        if field == "supplier" {
            m.supplier = (!bad.is_empty()).then(|| bad.into());
        } else {
            m.supplier_sku = Some(bad.into());
        }
        assert!(write_csv(&p, AssemblyCsv::CatalogBom, &path, true, None).is_err());
    }
    let mut p = fixture();
    let part = p.board.parts.values_mut().find(|p| p.assembly.as_ref().unwrap().populate).unwrap();
    part.assembly.as_mut().unwrap().mpn = None;
    assert!(write_csv(&p, AssemblyCsv::AssemblyBom, &path, true, None).unwrap_err().to_string().contains("manufacturer part number"));
    p.board.parts.values_mut().find(|p| p.assembly.as_ref().is_some_and(|m| m.populate)).unwrap().assembly = None;
    assert!(csv(&p, AssemblyCsv::CatalogBom).is_err());
    assert!(csv(&p, AssemblyCsv::AssemblyBom).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"keep");
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}
#[test]
fn excluded_missing_fields_empty_set_budgets_and_admission_are_explicit() {
    let _serial = SERIAL.lock().unwrap();
    let mut p = fixture();
    let excluded = p.board.parts.values_mut().find(|p| !p.assembly.as_ref().unwrap().populate).unwrap();
    excluded.designator = String::new();
    excluded.footprint = String::new();
    excluded.assembly.as_mut().unwrap().supplier = None;
    excluded.assembly.as_mut().unwrap().mpn = None;
    for kind in [AssemblyCsv::CatalogBom, AssemblyCsv::AssemblyBom] {
        assert!(csv(&p, kind).is_ok());
    }
    p.board.parts.values_mut().for_each(|p| p.assembly.as_mut().unwrap().populate = false);
    for kind in [AssemblyCsv::CatalogBom, AssemblyCsv::AssemblyBom] {
        let snapshot = AssemblySnapshot::capture_for(&p, kind).unwrap();
        assert_eq!(snapshot.component_count_for(kind), 0);
        assert_eq!(snapshot.csv(kind).unwrap().lines().count(), 1);
    }
    let mut p = fixture();
    p.board.parts.values_mut().next().unwrap().designator = "x".repeat(4097);
    assert!(csv(&p, AssemblyCsv::CatalogBom).unwrap_err().to_string().contains("4096"));
    let mut p = fixture();
    let template = p.board.parts.values().next().unwrap().clone();
    p.board.parts.clear();
    for index in 0..1000 {
        let mut part = template.clone();
        part.id = ObjectId::new();
        part.designator = format!("R{index}");
        part.value = "x".repeat(3000);
        p.board.parts.insert(part.id, part);
    }
    assert!(csv(&p, AssemblyCsv::CatalogBom).unwrap_err().to_string().contains("2 MiB"));
    let p = fixture();
    let first = AssemblySnapshot::capture(&p).unwrap();
    let second = AssemblySnapshot::capture(&p).unwrap();
    assert_eq!(AssemblySnapshot::capture_for(&p, AssemblyCsv::CatalogBom).err().unwrap().kind(), std::io::ErrorKind::WouldBlock);
    drop((first, second));
    assert!(csv(&p, AssemblyCsv::CatalogBom).is_ok());
}
