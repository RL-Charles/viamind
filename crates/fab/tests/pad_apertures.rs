use std::{fs, sync::Mutex};
use viamind_doc::{
    ApertureRect, PadApertures, Side,
    fabrication_view::{FabricationLayer as Layer, FabricationPrimitive as Primitive, FabricationReadBudget, parse_gerber},
    file,
};
use viamind_fab::{FabricationOptions, InspectOutputRelease, OutputJob, OutputJobSnapshot, ReleaseCursor, fabrication, inspect_output_release};
use viamind_geom::{Nm, Point};
static SERIAL: Mutex<()> = Mutex::new(());
fn sample() -> viamind_doc::Project {
    file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap()
}
fn parse(s: &str) -> viamind_doc::fabrication_view::FabricationGeometry {
    parse_gerber(s, &mut FabricationReadBudget::default()).unwrap()
}
#[test]
fn legacy_default_outputs_remain_identical_to_retained_pre_feature_binary() {
    let _lock = SERIAL.lock().unwrap();
    let p = sample();
    let out = fabrication(&p, FabricationOptions { mask_expansion_mm: 0.050001, ..Default::default() }).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fabrication/legacy-aperture-baseline");
    for (name, bytes) in &out.files {
        assert_eq!(*bytes, fs::read_to_string(root.join(name)).unwrap(), "{name}");
    }
    assert_eq!(out.files.len(), 7);
    assert!(out.manifest.pad_apertures.is_none());
    assert!(serde_json::to_value(FabricationOptions::default()).unwrap().get("include_paste").is_none());
}
#[test]
fn independent_rectangular_reader_all_cardinal_sides_absolute_mask_omission_and_polarity() {
    let _lock = SERIAL.lock().unwrap();
    let mut p = sample();
    p.board.vias.clear();
    let id = *p.board.parts.keys().next().unwrap();
    p.board.parts.retain(|key, _| *key == id);
    for (side, angle, offset) in [
        (Side::Top, 0, (3, 5)),
        (Side::Top, 90000, (5, -3)),
        (Side::Top, 180000, (-3, -5)),
        (Side::Top, 270000, (-5, 3)),
        (Side::Bottom, 0, (-3, 5)),
        (Side::Bottom, 90000, (5, 3)),
        (Side::Bottom, 180000, (3, -5)),
        (Side::Bottom, 270000, (-5, -3)),
    ] {
        let part = p.board.parts.get_mut(&id).unwrap();
        part.position = Point::new(Nm(10_000_000), Nm(20_000_000));
        part.rotation_mdeg = angle;
        part.side = side;
        part.pads.as_mut().unwrap().0.truncate(1);
        let pad = &mut part.pads.as_mut().unwrap().0[0];
        pad.position = Point::new(Nm(3), Nm(5));
        pad.apertures = Some(PadApertures {
            mask: Some(ApertureRect { width: Nm(7), height: Nm(11) }),
            paste: Some(ApertureRect { width: Nm(3), height: Nm(5) }),
        });
        let out = fabrication(&p, FabricationOptions { include_paste: true, mask_expansion_mm: 0.5, ..Default::default() }).unwrap();
        let (mask, paste, empty, layer) = if side == Side::Top {
            ("top-mask.gbr", "top-paste.gbr", "bottom-paste.gbr", Layer::TopPaste)
        } else {
            ("bottom-mask.gbr", "bottom-paste.gbr", "top-paste.gbr", Layer::BottomPaste)
        };
        let center = Point::new(Nm(10_000_000 + offset.0), Nm(20_000_000 + offset.1));
        let m = parse(&out.files[mask]);
        assert!(m.negative_file_polarity);
        assert_eq!(m.primitives, vec![Primitive::Rectangle { center, width: Nm(7), height: Nm(11), rotation_mdeg: angle }]);
        let a = parse(&out.files[paste]);
        assert_eq!(a.layer, layer);
        assert!(!a.negative_file_polarity);
        assert_eq!(a.same_coordinates, Some(p.id));
        assert_eq!(a.primitives, vec![Primitive::Rectangle { center, width: Nm(3), height: Nm(5), rotation_mdeg: angle }]);
        assert!(parse(&out.files[empty]).primitives.is_empty());
        assert!(
            parse_gerber(&out.files[paste].replace("FilePolarity,Positive", "FilePolarity,Negative"), &mut FabricationReadBudget::default()).is_err()
        );
        assert!(fabrication(&p, FabricationOptions::default()).err().unwrap().to_string().contains("include_paste"));
    }
    let part = p.board.parts.get_mut(&id).unwrap();
    part.pads.as_mut().unwrap().0[0].apertures = Some(PadApertures { mask: None, paste: None });
    let out = fabrication(&p, FabricationOptions { include_paste: true, include_silkscreen: true, ..Default::default() }).unwrap();
    assert!(parse(&out.files["bottom-mask.gbr"]).primitives.is_empty());
    assert!(parse(&out.files["bottom-paste.gbr"]).primitives.is_empty());
    assert!(!out.manifest.exclusions[1].contains("paste"));
    assert!(!out.manifest.exclusions[1].contains("silkscreen"));
    let summary = out.manifest.pad_apertures.unwrap();
    assert!(summary.explicit_coverage_complete);
    assert_eq!((summary.mask_omissions, summary.paste_omissions), (1, 1));
}
#[test]
fn version5_companion_matrix_exact_release_files_and_null_summary_refuse() {
    let _lock = SERIAL.lock().unwrap();
    let mut p = file::from_str(include_str!("../../../fixtures/assembly/output-job-population.viamind.json")).unwrap();
    for part in p.board.parts.values_mut() {
        for pad in &mut part.pads.as_mut().unwrap().0 {
            pad.apertures = Some(PadApertures { mask: None, paste: None });
        }
    }
    for silk in [false, true] {
        for (cal, pop) in [(false, false), (true, false), (true, true)] {
            let mut job = OutputJob::new("Explicit openings".into(), 0.0);
            job.version = 5;
            job.include_paste = Some(true);
            job.include_silkscreen = silk.then_some(true);
            job.include_calibrated_placement = cal.then_some(true);
            job.include_populated_bom = pop.then_some(true);
            assert_eq!(OutputJob::parse(&job.canonical_bytes().unwrap()).unwrap(), job);
            let temp = tempfile::tempdir().unwrap();
            let snapshot = OutputJobSnapshot::capture(&p, job.clone()).unwrap();
            let (path, manifest) =
                snapshot.write(ReleaseCursor { stream_id: viamind_doc::ObjectId::new(), sequence: 7 }, &temp.path().join("new"), None).unwrap();
            assert_eq!(manifest.files.len() + 1, 13 + usize::from(silk) * 2 + usize::from(cal) + usize::from(pop));
            inspect_output_release(&InspectOutputRelease { path: path.clone() }).unwrap_or_else(|e| panic!("silk={silk} cal={cal} pop={pop}: {e}"));
            let file = path.join("release-manifest.json");
            let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
            raw["pad_apertures"] = serde_json::Value::Null;
            fs::write(&file, serde_json::to_vec(&raw).unwrap()).unwrap();
            assert!(inspect_output_release(&InspectOutputRelease { path }).is_err());
            for flag in [serde_json::Value::Null, serde_json::json!(false)] {
                let mut raw = serde_json::to_value(&job).unwrap();
                raw["include_paste"] = flag;
                assert!(OutputJob::parse(&serde_json::to_vec(&raw).unwrap()).is_err());
            }
        }
    }
    let mut job = OutputJob::new("known membership".into(), 0.0);
    job.version = 5;
    job.include_paste = Some(true);
    let id = *p.board.parts.keys().next().unwrap();
    p.board.parts.get_mut(&id).unwrap().pads = None;
    assert!(OutputJobSnapshot::capture(&p, job.clone()).err().unwrap().to_string().contains("membership"));
    p.board.parts.get_mut(&id).unwrap().pads = Some(viamind_doc::Pads::try_from(vec![]).unwrap());
    assert!(OutputJobSnapshot::capture(&p, job).is_ok());
    for version in 1..=4 {
        let mut job = OutputJob::new("legacy".into(), 0.0);
        job.version = version;
        job.include_calibrated_placement = (version == 2 || version == 3).then_some(true);
        job.include_populated_bom = (version == 3).then_some(true);
        job.include_silkscreen = (version == 4).then_some(true);
        assert!(OutputJobSnapshot::capture(&p, job).is_err());
    }
}

fn change_release_file(path: &std::path::Path, name: &str, mutate: impl FnOnce(String) -> String) {
    let file = path.join(name);
    let bytes = mutate(fs::read_to_string(&file).unwrap());
    fs::write(&file, bytes.as_bytes()).unwrap();
    let manifest_path = path.join("release-manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["files"][name] = serde_json::json!(bytes.len());
    fs::write(manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
}
#[test]
fn independent_job5_observed_openings_refuse_semantic_corruption_even_when_lengths_agree() {
    let _lock = SERIAL.lock().unwrap();
    let mut p = sample();
    for part in p.board.parts.values_mut() {
        for pad in &mut part.pads.as_mut().unwrap().0 {
            pad.apertures = Some(PadApertures {
                mask: Some(ApertureRect { width: Nm(7), height: Nm(11) }),
                paste: Some(ApertureRect { width: Nm(3), height: Nm(5) }),
            });
        }
    }
    let mut job = OutputJob::new("Independent opening observations".into(), 0.0);
    job.version = 5;
    job.include_paste = Some(true);
    let tmp = tempfile::tempdir().unwrap();
    for n in 0..5 {
        let snapshot = OutputJobSnapshot::capture(&p, job.clone()).unwrap();
        let (path, manifest) = snapshot
            .write(ReleaseCursor { stream_id: viamind_doc::ObjectId::new(), sequence: 0 }, &tmp.path().join(format!("case-{n}")), None)
            .unwrap();
        let input = InspectOutputRelease { path: path.clone() };
        let report = inspect_output_release(&input).unwrap();
        let observed = report.observed_pad_openings.unwrap();
        let summary = manifest.pad_apertures.as_ref().unwrap();
        assert_eq!(observed.top_mask_rectangles, summary.top_mask_rectangles);
        assert_eq!(observed.bottom_mask_rectangles, summary.bottom_mask_rectangles);
        assert_eq!(observed.top_paste_rectangles, summary.top_paste_rectangles);
        assert_eq!(observed.bottom_paste_rectangles, summary.bottom_paste_rectangles);
        assert_eq!(observed.top_mask_via_circles, p.board.vias.len());
        assert_eq!(observed.bottom_mask_via_circles, p.board.vias.len());
        match n {
            0 => change_release_file(&path, "fabrication/top-mask.gbr", |s| {
                let changed = s.replace("Soldermask,Top", "Soldermask,Bot");
                assert_ne!(s, changed);
                assert_eq!(s.len(), changed.len());
                changed
            }),
            1 => change_release_file(&path, "fabrication/top-paste.gbr", |s| {
                let changed = s.replace("FilePolarity,Positive", "FilePolarity,Negative");
                assert_ne!(s, changed);
                assert_eq!(s.len(), changed.len());
                changed
            }),
            2 => change_release_file(&path, "fabrication/bottom-paste.gbr", |s| {
                let changed = s.replace(&p.id.to_string(), &viamind_doc::ObjectId::new().to_string());
                assert_ne!(s, changed);
                assert_eq!(s.len(), changed.len());
                changed
            }),
            3 => {
                let side = if summary.top_paste_rectangles > 0 { "top" } else { "bottom" };
                change_release_file(&path, &format!("fabrication/{side}-paste.gbr"), |s| {
                    let flash = s.lines().find(|line| line.starts_with('X') && line.ends_with("D03*")).unwrap();
                    s.replacen(&format!("{flash}\n"), "", 1)
                });
            }
            _ => change_release_file(&path, "fabrication/manifest.json", |s| {
                let mut v: serde_json::Value = serde_json::from_str(&s).unwrap();
                v["through_via_count"] = serde_json::json!(p.board.vias.len() + 1);
                serde_json::to_string(&v).unwrap()
            }),
        }
        assert!(inspect_output_release(&input).is_err(), "corruption {n} accepted despite matching declared byte length");
    }
}
