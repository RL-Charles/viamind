use std::{fs, path::Path, sync::Mutex};
use viamind_doc::{ObjectId, PadApertures, footprint_holes::PartHole};
use viamind_fab::{InspectOutputRelease, OutputJob, OutputJobSnapshot, ReleaseCursor, inspect_output_release};
use viamind_geom::{Nm, Point};
static SERIAL: Mutex<()> = Mutex::new(());
fn source() -> viamind_doc::Project {
    let mut p = viamind_doc::file::from_str(include_str!("../../../fixtures/assembly/output-job-population.viamind.json")).unwrap();
    p.board.parts.values_mut().next().unwrap().holes =
        Some(vec![PartHole { id: ObjectId::new(), key: "mount".into(), center: Point::new(Nm(3), Nm(5)), diameter: Nm(1_000_001) }]);
    p
}
fn job() -> OutputJob {
    let mut j = OutputJob::new("Explicit mechanical drills".into(), 0.0);
    j.version = 6;
    j.include_npth = Some(true);
    j
}
#[test]
fn all_twenty_four_job6_compositions_have_independent_drill_observations() {
    let _lock = SERIAL.lock().unwrap();
    for paste in [false, true] {
        let mut p = source();
        for part in p.board.parts.values_mut() {
            for pad in &mut part.pads.as_mut().unwrap().0 {
                pad.apertures = paste.then_some(PadApertures { mask: None, paste: None });
            }
        }
        for checked in [false, true] {
            for silk in [false, true] {
                for (cal, pop) in [(false, false), (true, false), (true, true)] {
                    let mut j = job();
                    j.include_paste = paste.then_some(true);
                    j.include_silkscreen = silk.then_some(true);
                    j.include_calibrated_placement = cal.then_some(true);
                    j.include_populated_bom = pop.then_some(true);
                    if checked {
                        j.profile = viamind_fab::CHECKED_PROFILE.into();
                        j.checks = Some(serde_json::from_value(serde_json::json!({"copper_clearance_mm":0,"edge_clearance_mm":0,"minimum_via_drill_mm":0,"minimum_annular_width_mm":0,"hole_clearance_mm":0,"hole_to_copper_clearance_mm":0,"max_violations":100})).unwrap());
                    }
                    assert_eq!(OutputJob::parse(&j.canonical_bytes().unwrap()).unwrap(), j);
                    let temp = tempfile::tempdir().unwrap();
                    let (path, manifest) = OutputJobSnapshot::capture(&p, j)
                        .unwrap()
                        .write_with_validation(
                            ReleaseCursor { stream_id: ObjectId::new(), sequence: 7 },
                            &temp.path().join("release"),
                            None,
                            |_, _, _| Ok(b"opaque unit fixture, not production DRC evidence".to_vec()),
                        )
                        .unwrap();
                    assert_eq!(
                        manifest.files.len() + 1,
                        12 + 2 * usize::from(paste) + 2 * usize::from(silk) + usize::from(cal) + usize::from(pop) + usize::from(checked)
                    );
                    let report = inspect_output_release(&InspectOutputRelease { path })
                        .unwrap_or_else(|e| panic!("paste={paste} checked={checked} silk={silk} cal={cal} pop={pop}: {e}"));
                    let facts = report.observed_nonplated_holes.unwrap();
                    assert_eq!((facts.physical_drills, facts.tool_diameters, facts.plated_via_drills), (1, 1, p.board.vias.len()));
                    assert_eq!(facts.same_coordinates, p.id);
                    assert_eq!(report.observed_pad_openings.is_some(), paste);
                }
            }
        }
    }
}
#[test]
fn job6_requires_explicit_npth_and_preserves_other_versions_and_aperture_admission() {
    let _lock = SERIAL.lock().unwrap();
    for flag in [None, Some(serde_json::json!(false)), Some(serde_json::Value::Null)] {
        let mut raw = serde_json::to_value(job()).unwrap();
        raw.as_object_mut().unwrap().remove("include_npth");
        if let Some(flag) = flag {
            raw["include_npth"] = flag;
        }
        assert!(OutputJob::parse(&serde_json::to_vec(&raw).unwrap()).is_err());
    }
    for version in 1..=5 {
        for flag in [serde_json::json!(true), serde_json::json!(false), serde_json::Value::Null] {
            let mut raw = serde_json::to_value(job()).unwrap();
            raw["version"] = version.into();
            raw["include_npth"] = flag;
            assert!(OutputJob::parse(&serde_json::to_vec(&raw).unwrap()).is_err());
        }
    }
    for field in ["include_paste", "include_silkscreen", "include_calibrated_placement", "include_populated_bom"] {
        for flag in [serde_json::json!(false), serde_json::Value::Null] {
            let mut raw = serde_json::to_value(job()).unwrap();
            raw[field] = flag;
            assert!(OutputJob::parse(&serde_json::to_vec(&raw).unwrap()).is_err());
        }
    }
    let mut j = job();
    j.include_populated_bom = Some(true);
    assert!(j.canonical_bytes().is_err());
    j = job();
    j.include_paste = Some(true);
    let mut p = source();
    assert!(OutputJobSnapshot::capture(&p, j.clone()).is_err());
    for part in p.board.parts.values_mut() {
        for pad in &mut part.pads.as_mut().unwrap().0 {
            pad.apertures = Some(PadApertures { mask: None, paste: None });
        }
    }
    assert!(OutputJobSnapshot::capture(&p, j.clone()).is_ok());
    assert!(OutputJobSnapshot::capture(&p, job()).is_err());
    p.board.parts.values_mut().next().unwrap().pads = None;
    assert!(OutputJobSnapshot::capture(&p, j).is_err());
    p = source();
    p.board.parts.values_mut().next().unwrap().holes = Some(vec![]);
    let temp = tempfile::tempdir().unwrap();
    let (path, _) = OutputJobSnapshot::capture(&p, job())
        .unwrap()
        .write(ReleaseCursor { stream_id: ObjectId::new(), sequence: 0 }, &temp.path().join("empty"), None)
        .unwrap();
    let facts = inspect_output_release(&InspectOutputRelease { path }).unwrap().observed_nonplated_holes.unwrap();
    assert_eq!((facts.physical_drills, facts.tool_diameters), (0, 0));
}
fn change(path: &Path, name: &str, mutate: impl FnOnce(String) -> String) {
    let file = path.join(name);
    let before = fs::read_to_string(&file).unwrap();
    let after = mutate(before.clone());
    assert_ne!(before, after);
    fs::write(file, after.as_bytes()).unwrap();
    let file = path.join("release-manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    manifest["files"][name] = serde_json::json!(after.len());
    fs::write(file, serde_json::to_vec(&manifest).unwrap()).unwrap();
}
#[test]
fn drill_semantic_corruption_refuses_even_after_file_lengths_are_updated() {
    let _lock = SERIAL.lock().unwrap();
    let p = source();
    let temp = tempfile::tempdir().unwrap();
    for case in 0..7 {
        let (path, _) = OutputJobSnapshot::capture(&p, job())
            .unwrap()
            .write(ReleaseCursor { stream_id: ObjectId::new(), sequence: 0 }, &temp.path().join(format!("case{case}")), None)
            .unwrap();
        inspect_output_release(&InspectOutputRelease { path: path.clone() }).unwrap();
        let name = "fabrication/nonplated-through.xnc";
        match case {
            0 => change(&path, name, |s| s.replace("NonPlated,1,2,NPTH", "Plated,1,2,PTH")),
            1 => change(&path, name, |s| s.replace("NonPlated,1,2,NPTH", "NonPlated,1,3,NPTH")),
            2 => change(&path, name, |s| s.replace(&p.id.to_string(), &ObjectId::new().to_string())),
            3 => change(&path, name, |s| s.replace("\nT01\n", "\nT02\n")),
            4 => change(&path, name, |s| s.replace("T01C1.000001\n", "T01C1.000001\nT02C1.000001\n")),
            5 => change(&path, name, |s| s.lines().filter(|l| !l.starts_with('X')).collect::<Vec<_>>().join("\n") + "\n"),
            6 => change(&path, "fabrication/plated-through.xnc", |s| s.lines().filter(|l| !l.starts_with('X')).collect::<Vec<_>>().join("\n") + "\n"),
            _ => unreachable!(),
        }
        let before: Vec<_> = path
            .read_dir()
            .unwrap()
            .map(|e| {
                let p = e.unwrap().path();
                (p.clone(), if p.is_file() { fs::read(p).unwrap() } else { vec![] })
            })
            .collect();
        assert!(inspect_output_release(&InspectOutputRelease { path }).is_err(), "case{case}");
        for (file, bytes) in before {
            if file.is_file() {
                assert_eq!(fs::read(file).unwrap(), bytes);
            }
        }
    }
}
