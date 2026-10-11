use serde_json::json;
use viamind_doc::{
    FootprintArtwork, SilkscreenSegment, file,
    footprint::{self, Footprint},
};
use viamind_geom::{Nm, Point};

#[test]
fn strict_library_versions_explicit_empty_and_legacy_bytes() {
    let old = Footprint { artwork: None, name: "Empty pads".into(), pads: vec![] };
    let legacy = footprint::to_vec(&old).unwrap();
    assert_eq!(footprint::from_slice(&legacy).unwrap(), old);
    assert!(!String::from_utf8(legacy.clone()).unwrap().contains("artwork"));
    let explicit = Footprint { artwork: Some(Box::default()), ..old.clone() };
    let bytes = footprint::to_vec(&explicit).unwrap();
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["version"], 2);
    assert_eq!(footprint::from_slice(&bytes).unwrap(), explicit);
    for artwork in [None, Some(json!(null))] {
        let mut input: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        if let Some(value) = artwork {
            input["footprint"]["artwork"] = value;
        } else {
            input["footprint"].as_object_mut().unwrap().remove("artwork");
        }
        assert!(footprint::from_slice(&serde_json::to_vec(&input).unwrap()).unwrap_err().contains("non-null"));
    }
    for artwork in [json!(null), json!({}), json!({"silkscreen":[]})] {
        let mut input: serde_json::Value = serde_json::from_slice(&legacy).unwrap();
        input["footprint"]["artwork"] = artwork;
        assert!(footprint::from_slice(&serde_json::to_vec(&input).unwrap()).unwrap_err().contains("missing"));
    }
    let empty: FootprintArtwork = serde_json::from_value(json!({})).unwrap();
    assert_eq!(empty, FootprintArtwork::default());
    let unknown = json!({"silkscreen":[{"start":{"x":0,"y":0,"unknown":1},"end":{"x":3,"y":4},"width":1}]});
    assert!(serde_json::from_value::<FootprintArtwork>(unknown).is_err());
}

#[test]
fn native_feature_max_save_and_private_analysis_never_drop_live_artwork() {
    let mut p = file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let original = file::to_string(&p);
    let id = *p.board.parts.keys().next().unwrap();
    p.board.parts.get_mut(&id).unwrap().artwork = Some(Box::default());
    assert_eq!(file::minimum_version(&p), 13);
    let text = file::to_string(&p);
    assert_eq!(file::from_str(&text).unwrap(), p);
    assert!(file::from_str(&text.replace("\"version\": 13", "\"version\": 12")).is_err());
    assert_eq!(p.board.clone_without_assembly().parts[&id].artwork, p.board.parts[&id].artwork);
    assert!(p.board.clone_for_electrical_analysis().parts[&id].artwork.is_none());
    p.board.parts.get_mut(&id).unwrap().artwork = None;
    assert_eq!(file::to_string(&p), original);
    let art = FootprintArtwork {
        silkscreen: vec![SilkscreenSegment { start: Point::new(Nm(0), Nm(0)), end: Point::new(Nm(3), Nm(5)), width: Nm(1) }; 100_000],
        courtyard: None,
    };
    p.board.parts.get_mut(&id).unwrap().artwork = Some(Box::new(art));
    let shell = p.board.clone_for_electrical_analysis();
    assert!(shell.parts[&id].artwork.is_none());
    assert_eq!(p.board.parts[&id].artwork.as_ref().unwrap().silkscreen.len(), 100_000);
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("invalid.json");
    assert!(file::save_new(&p, &path).is_err());
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
}

#[test]
fn mixed_artwork_package_via_and_assembly_features_max_compose_and_clear() {
    use viamind_doc::{AssemblyMetadata, MountingTechnology, NamedNetMinima};
    let mut p = file::from_str(include_str!("../../../docs/verification/t0116/placed.viamind.json")).unwrap();
    assert_eq!(file::minimum_version(&p), 11, "historical fixture places schematic units only");
    let mut part =
        file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap().board.parts.into_values().next().unwrap();
    part.id = viamind_doc::ObjectId::new();
    for pad in &mut part.pads.as_mut().unwrap().0 {
        pad.id = viamind_doc::ObjectId::new();
    }
    part.schematic_package = Some(*p.schematic.sheets.values().next().unwrap().content.as_ref().unwrap().packages.keys().next().unwrap());
    let id = part.id;
    p.board.parts.insert(id, part);
    assert_eq!(file::minimum_version(&p), 12);
    p.board.parts.get_mut(&id).unwrap().assembly =
        Some(Box::new(AssemblyMetadata { mounting: Some(MountingTechnology::Hybrid), ..Default::default() }));
    p.board.net_rules.insert(
        "Exact".into(),
        NamedNetMinima { minimum_copper_clearance: Nm(0), minimum_track_width: Nm(0), minimum_via_drill: Nm(3), minimum_annular_width: Nm(5) },
    );
    p.board.parts.get_mut(&id).unwrap().artwork = Some(Box::default());
    assert_eq!(file::minimum_version(&p), 13);
    assert_eq!(file::from_str(&file::to_string(&p)).unwrap(), p);
    p.board.parts.get_mut(&id).unwrap().artwork = None;
    assert_eq!(file::minimum_version(&p), 12);
    for part in p.board.parts.values_mut() {
        part.schematic_package = None;
        part.schematic_symbol = None;
    }
    p.schematic = Default::default();
    assert_eq!(file::minimum_version(&p), 10);
    p.board.net_rules.clear();
    assert_eq!(file::minimum_version(&p), 9);
    p.board.parts.get_mut(&id).unwrap().assembly = None;
    assert!(file::minimum_version(&p) < 9);
    assert_eq!(file::from_str(&file::to_string(&p)).unwrap(), p);
}
