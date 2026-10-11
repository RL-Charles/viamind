use serde_json::json;
use std::{fs, sync::Mutex};
use viamind_doc::{
    FootprintArtwork, RectangularCourtyard, Side, SilkscreenSegment,
    fabrication_view::{FabricationLayer, FabricationPrimitive, FabricationReadBudget, parse_gerber},
    file,
};
use viamind_fab::{
    FabricationOptions, FabricationSnapshot, InspectOutputRelease, OutputJob, OutputJobSnapshot, ReleaseCursor, fabrication, inspect_output_release,
    write_fabrication,
};
use viamind_geom::{Nm, Point};
static SERIAL: Mutex<()> = Mutex::new(());
fn project() -> viamind_doc::Project {
    file::from_str(include_str!("../../../fixtures/assembly/output-job-population.viamind.json")).unwrap()
}
fn art() -> Box<FootprintArtwork> {
    Box::new(FootprintArtwork {
        silkscreen: vec![SilkscreenSegment { start: Point::new(Nm(-3), Nm(5)), end: Point::new(Nm(7), Nm(11)), width: Nm(3) }],
        courtyard: Some(RectangularCourtyard { min: Point::new(Nm(-13), Nm(-17)), max: Point::new(Nm(19), Nm(23)), width: Nm(5) }),
    })
}
#[test]
fn independent_legend_reader_all_cardinal_mirrors_arbitrary_rounding_and_old_files() {
    let _serial = SERIAL.lock().unwrap();
    let mut p = project();
    let id = *p.board.parts.keys().next().unwrap();
    let baseline = fabrication(&p, FabricationOptions::default()).unwrap();
    assert!(baseline.manifest.footprint_artwork.is_none());
    assert!(serde_json::to_value(FabricationOptions::default()).unwrap().get("include_silkscreen").is_none());
    for (side, angle, start, end) in [
        (Side::Top, 0, (-3, 5), (7, 11)),
        (Side::Top, 90000, (5, 3), (11, -7)),
        (Side::Top, 180000, (3, -5), (-7, -11)),
        (Side::Top, 270000, (-5, -3), (-11, 7)),
        (Side::Bottom, 0, (3, 5), (-7, 11)),
        (Side::Bottom, 90000, (5, -3), (11, 7)),
        (Side::Bottom, 180000, (-3, -5), (7, -11)),
        (Side::Bottom, 270000, (-5, 3), (-11, -7)),
    ] {
        let part = p.board.parts.get_mut(&id).unwrap();
        part.position = Point::new(Nm(10_000_000), Nm(20_000_000));
        part.side = side;
        part.rotation_mdeg = angle;
        part.artwork = Some(art());
        let package = fabrication(&p, FabricationOptions { include_silkscreen: true, ..Default::default() }).unwrap();
        let (name, layer, empty) = if side == Side::Top {
            ("top-silkscreen.gbr", FabricationLayer::TopSilkscreen, "bottom-silkscreen.gbr")
        } else {
            ("bottom-silkscreen.gbr", FabricationLayer::BottomSilkscreen, "top-silkscreen.gbr")
        };
        let parsed = parse_gerber(&package.files[name], &mut FabricationReadBudget::default()).unwrap();
        assert_eq!(parsed.layer, layer);
        assert!(!parsed.negative_file_polarity);
        assert_eq!(parsed.same_coordinates, Some(p.id));
        assert_eq!(
            parsed.primitives,
            vec![FabricationPrimitive::Segment {
                start: Point::new(Nm(10_000_000 + start.0), Nm(20_000_000 + start.1)),
                end: Point::new(Nm(10_000_000 + end.0), Nm(20_000_000 + end.1)),
                width: Nm(3)
            }]
        );
        assert!(parse_gerber(&package.files[empty], &mut FabricationReadBudget::default()).unwrap().primitives.is_empty());
        assert_eq!(package.manifest.footprint_artwork.as_ref().unwrap().courtyard_count, 1);
        assert_eq!(package.files.len(), 9);
        assert!(
            parse_gerber(&package.files[name].replace("D01*", "D03*"), &mut FabricationReadBudget::default()).is_err(),
            "Legend flashes must refuse"
        );
    }
    // Independent Python math reference, rotation12.345deg, before nearest-Nm rounding:
    // Top (8138352.7115151575,25525788.393823184), (19189929.49916153,29249082.97079287).
    // Bottom (13999626.329380533,24243000.96928577), (5513624.390808988,32242253.628046837).
    for (side, expected) in
        [(Side::Top, [(8_138_353, 25_525_788), (19_189_929, 29_249_083)]), (Side::Bottom, [(13_999_626, 24_243_001), (5_513_624, 32_242_254)])]
    {
        let part = p.board.parts.get_mut(&id).unwrap();
        part.side = side;
        part.rotation_mdeg = 12345;
        let line = &mut part.artwork.as_mut().unwrap().silkscreen[0];
        line.start = Point::new(Nm(-3_000_003), Nm(5_000_005));
        line.end = Point::new(Nm(7_000_007), Nm(11_000_011));
        let package = fabrication(&p, FabricationOptions { include_silkscreen: true, ..Default::default() }).unwrap();
        let name = if side == Side::Top { "top-silkscreen.gbr" } else { "bottom-silkscreen.gbr" };
        let parsed = parse_gerber(&package.files[name], &mut FabricationReadBudget::default()).unwrap();
        assert_eq!(
            parsed.primitives,
            vec![FabricationPrimitive::Segment {
                start: Point::new(Nm(expected[0].0), Nm(expected[0].1)),
                end: Point::new(Nm(expected[1].0), Nm(expected[1].1)),
                width: Nm(3)
            }]
        );
    }
    let mut p = project();
    p.board.parts.get_mut(&id).unwrap().artwork = Some(art());
    let excluded = fabrication(&p, FabricationOptions::default()).unwrap();
    for (name, bytes) in baseline.files.iter().filter(|(name, _)| name.as_str() != "manifest.json") {
        assert_eq!(&excluded.files[name], bytes);
    }
    assert_eq!(excluded.files.len(), 7);
    assert!(!excluded.manifest.footprint_artwork.unwrap().include_silkscreen);
}
#[test]
fn job_four_exact_companion_matrix_captured_art_and_strict_release_consistency() {
    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    for (calibrated, populated) in [(false, false), (true, false), (true, true)] {
        let mut p = project();
        let id = *p.board.parts.keys().next().unwrap();
        p.board.parts.get_mut(&id).unwrap().artwork = Some(art());
        let mut job = OutputJob::new("Explicit Legend".into(), 0.);
        job.version = 4;
        job.include_silkscreen = Some(true);
        job.include_calibrated_placement = calibrated.then_some(true);
        job.include_populated_bom = populated.then_some(true);
        assert_eq!(OutputJob::parse(&job.canonical_bytes().unwrap()).unwrap(), job);
        let snapshot = OutputJobSnapshot::capture(&p, job.clone()).unwrap();
        p.board.parts.clear();
        let cursor = ReleaseCursor { stream_id: viamind_doc::ObjectId::new(), sequence: 17 };
        let (path, manifest) = snapshot.write(cursor, &temp.path().join(format!("{calibrated}-{populated}")), None).unwrap();
        assert_eq!(manifest.files.len() + 1, 13 + usize::from(calibrated) + usize::from(populated));
        assert_eq!(manifest.footprint_artwork.as_ref().unwrap().explicit_components, 1);
        assert_eq!(manifest.exported_cursor, cursor);
        let inspected = inspect_output_release(&InspectOutputRelease { path: path.clone() }).unwrap();
        assert_eq!(inspected.file_count, manifest.files.len() + 1);
        assert_eq!(inspected.exported_cursor, cursor);
        let file = path.join("release-manifest.json");
        let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        value["footprint_artwork"]["top_silkscreen_segments"] = json!(199999);
        fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(inspect_output_release(&InspectOutputRelease { path }).is_err(), "summary mismatch must refuse");
        for version in 1..=3 {
            let mut old = job.clone();
            old.version = version;
            assert!(old.validate().is_err());
        }
        for flag in [None, Some(false)] {
            let mut invalid = job.clone();
            invalid.include_silkscreen = flag;
            assert!(invalid.validate().is_err());
        }
        let mut invalid = job;
        invalid.include_calibrated_placement = None;
        invalid.include_populated_bom = Some(true);
        assert!(invalid.validate().is_err());
    }
    let mut value = serde_json::to_value(OutputJob::new("Legacy".into(), 0.)).unwrap();
    value["version"] = json!(4);
    value["include_silkscreen"] = json!(null);
    assert!(OutputJob::parse(&serde_json::to_vec(&value).unwrap()).is_err());
}
#[test]
fn borrowed_artwork_capture_refusal_preserves_source_and_creates_no_stage() {
    let _serial = SERIAL.lock().unwrap();
    let mut p = project();
    let id = *p.board.parts.keys().next().unwrap();
    let mut excessive = art();
    excessive.silkscreen = vec![excessive.silkscreen[0].clone(); 100000];
    p.board.parts.get_mut(&id).unwrap().artwork = Some(excessive);
    let temp = tempfile::tempdir().unwrap();
    assert!(FabricationSnapshot::capture(&p).is_err());
    assert!(write_fabrication(&p, FabricationOptions { include_silkscreen: true, ..Default::default() }, &temp.path().join("never"), None).is_err());
    assert_eq!(p.board.parts[&id].artwork.as_ref().unwrap().silkscreen.len(), 100000);
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
}
