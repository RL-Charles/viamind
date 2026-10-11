use quick_xml::{Reader, events::Event};
use std::{
    fs,
    sync::{Arc, Barrier},
};
use viamind_doc::{Project, file};
use viamind_render::{SvgOptions, ViewBounds, ViewLayer, board_svg, write_svg};
fn sample() -> Project {
    file::from_str(include_str!("../../../fixtures/views/asymmetric.viamind.json")).unwrap()
}
#[test]
fn direct_views_and_captures_exclude_unrelated_sourcing_before_copy() {
    let mut project = sample();
    let expected = board_svg(&project, &SvgOptions::default()).unwrap();
    for part in project.board.parts.values_mut() {
        part.assembly = Some(viamind_doc::AssemblyMetadata { mpn: Some("unused".repeat(400000)), populate: false, ..Default::default() });
    }
    let snapshot = viamind_render::board_view_snapshot(&project).unwrap();
    assert!(snapshot.board.parts.values().all(|p| p.assembly.is_none()));
    assert_eq!(board_svg(&project, &SvgOptions::default()).unwrap().svg, expected.svg);
    assert_eq!(board_svg(&snapshot, &SvgOptions::default()).unwrap().svg, expected.svg);
}
#[test]
fn transformed_geometry_exact_half_nanometres_and_stable_layers() {
    let project = sample();
    let all = board_svg(&project, &SvgOptions::default()).unwrap();
    assert_eq!(all.svg, board_svg(&project, &SvgOptions::default()).unwrap().svg);
    assert_eq!(all.metadata.region_nm, ViewBounds { min_x: -1_050_000, min_y: -1_050_000, max_x: 61_050_000, max_y: 41_050_000 });
    assert_eq!(all.metadata.pad_count, 3);
    assert_eq!(all.metadata.track_count, 2);
    assert!(all.svg.contains("translate(29.0179490 19.2990380) rotate(-30.000)"));
    assert!(all.svg.contains("stroke-linecap=\"round\""));
    assert!(all.svg.contains("fill-rule=\"evenodd\""));
    assert!(all.svg.find("layer-Outline").unwrap() < all.svg.find("layer-BottomCopper").unwrap());
    assert!(all.svg.find("layer-BottomCopper").unwrap() < all.svg.find("layer-TopCopper").unwrap());
    let mut project = project;
    let via = project.board.vias.values_mut().next().unwrap();
    via.diameter.0 = 3;
    via.drill.0 = 1;
    let odd = board_svg(&project, &SvgOptions::default()).unwrap();
    assert!(odd.svg.contains("r=\"0.0000015\""));
    assert!(odd.svg.contains("r=\"0.0000005\" fill=\"black\""));
}
#[test]
fn region_layers_deduplicate_and_empty_fit_is_declared() {
    let region = ViewBounds { min_x: 10_000_000, min_y: 20_000_000, max_x: 20_000_000, max_y: 30_000_000 };
    let options = SvgOptions {
        layers: vec![ViewLayer::TopCopper, ViewLayer::TopCopper],
        region_nm: Some(region),
        width_px: 700,
        height_px: 350,
        padding_nm: 0,
    };
    let view = board_svg(&sample(), &options).unwrap();
    assert_eq!(view.metadata.region_nm, region);
    assert_eq!(view.metadata.layers, vec![ViewLayer::TopCopper]);
    assert_eq!(view.metadata.pad_count, 1);
    assert_eq!(view.metadata.track_count, 1);
    assert!(!view.svg.contains("layer-BottomCopper"));
    assert!(!view.svg.contains("board-outline"));
    assert!(view.svg.contains("viewBox=\"10.0000000 20.0000000 10.0000000 10.0000000\""));
    assert!(view.svg.contains("clip-path=\"url(#view-clip)\""));
    let mut one = options;
    one.layers.pop();
    assert_eq!(view.svg, board_svg(&sample(), &one).unwrap().svg);
    let mut project = Project::new("Empty");
    let empty = board_svg(&project, &SvgOptions::default()).unwrap();
    assert_eq!(empty.metadata.region_nm, ViewBounds { min_x: -5_000_000, min_y: -5_000_000, max_x: 5_000_000, max_y: 5_000_000 });
    let mut legacy = sample().board.parts.values().next().unwrap().clone();
    legacy.pads = None;
    project.board.parts.insert(legacy.id, legacy);
    let omitted = board_svg(&project, &SvgOptions::default()).unwrap();
    assert_eq!(omitted.metadata.omitted_legacy_parts, 1);
    assert_eq!(omitted.metadata.pad_count, 0);
    assert_eq!(omitted.metadata.region_nm, empty.metadata.region_nm);
}
#[test]
fn xml_unicode_entities_controls_and_no_active_content_roundtrip_parse() {
    let mut project = sample();
    project.name = "<script onload=\"bad\">& 'café 🧪'\r\n\t\u{1}".into();
    project.board.parts.values_mut().next().unwrap().value = "</title><image href=\"https://bad.example/\"/>".into();
    let view = board_svg(&project, &SvgOptions::default()).unwrap();
    assert!(view.metadata.replaced_invalid_xml_text);
    assert!(view.svg.contains("&lt;script onload=&quot;bad&quot;&gt;&amp; &apos;café 🧪&apos;&#13;\n\t�"));
    let mut reader = Reader::from_str(&view.svg);
    let mut titles = 0;
    loop {
        match reader.read_event().unwrap() {
            Event::Start(e) | Event::Empty(e) => {
                let n = e.local_name();
                assert!(
                    [b"svg".as_slice(), b"title", b"desc", b"defs", b"clipPath", b"rect", b"mask", b"circle", b"g", b"path"].contains(&n.as_ref())
                );
                if n.as_ref() == b"title" {
                    titles += 1;
                }
                for a in e.attributes() {
                    let a = a.unwrap();
                    assert!(!a.key.as_ref().starts_with(b"on"));
                    assert_ne!(a.key.as_ref(), b"href");
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    assert!(titles >= 8);
}
#[test]
fn invalid_requests_models_metadata_and_bounds_are_rejected() {
    for options in [
        SvgOptions { layers: vec![], ..Default::default() },
        SvgOptions { width_px: 0, ..Default::default() },
        SvgOptions { height_px: 4097, ..Default::default() },
        SvgOptions { padding_nm: -1, ..Default::default() },
        SvgOptions { region_nm: Some(ViewBounds { min_x: 0, min_y: 0, max_x: 0, max_y: 1 }), ..Default::default() },
        SvgOptions { region_nm: Some(ViewBounds { min_x: i64::MIN, min_y: 0, max_x: 1, max_y: 1 }), ..Default::default() },
    ] {
        assert!(board_svg(&sample(), &options).is_err());
    }
    assert!(serde_json::from_value::<SvgOptions>(serde_json::json!({"layers":["inner_copper"]})).is_err());
    let mut p = sample();
    p.board.vias.values_mut().next().unwrap().drill.0 = 0;
    assert!(board_svg(&p, &SvgOptions::default()).is_err());
    p = sample();
    p.board.parts.values_mut().next().unwrap().position.x.0 = i64::MAX;
    assert!(board_svg(&p, &SvgOptions::default()).is_err());
    p = sample();
    p.name = "a".repeat(4097);
    assert!(board_svg(&p, &SvgOptions::default()).unwrap_err().to_string().contains("4096"));
    p = sample();
    let v = p.board.vias.values().next().unwrap().clone();
    for _ in 0..33334 {
        let mut v = v.clone();
        v.id = viamind_doc::ObjectId::new();
        p.board.vias.insert(v.id, v);
    }
    assert!(board_svg(&p, &SvgOptions::default()).unwrap_err().to_string().contains("100000 primitives"));
}
#[test]
fn publication_preserves_native_collisions_links_and_concurrent_winner() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("view.svg");
    let svg = board_svg(&sample(), &SvgOptions::default()).unwrap().svg;
    fs::write(&path, "native").unwrap();
    assert!(write_svg(&svg, &path, Some(&path)).unwrap_err().to_string().contains("active native"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "native");
    assert!(write_svg(&svg, &path, None).is_err());
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(write_svg(&svg, &path, None).is_err());
    fs::remove_dir(&path).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path().join("missing"), &path).unwrap();
        assert!(write_svg(&svg, &path, None).is_err());
        assert!(path.is_symlink());
        fs::remove_file(&path).unwrap();
    }
    let barrier = Arc::new(Barrier::new(2));
    let jobs: Vec<_> = (0..2)
        .map(|_| {
            let b = barrier.clone();
            let p = path.clone();
            let s = svg.clone();
            std::thread::spawn(move || {
                b.wait();
                write_svg(&s, &p, None)
            })
        })
        .collect();
    assert_eq!(jobs.into_iter().filter_map(|j| j.join().unwrap().ok()).count(), 1);
    assert_eq!(fs::read_to_string(path).unwrap(), svg);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
