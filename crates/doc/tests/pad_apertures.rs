use viamind_doc::{
    ApertureRect, ObjectId, Pad, PadApertures, Pads, Part, Project, Side, file,
    footprint::{Footprint, FootprintPad, LocalPosition},
};
use viamind_geom::{Nm, Point};
fn pad() -> Pad {
    Pad { id: ObjectId::new(), number: "1".into(), position: Point::ORIGIN, width: Nm(5), height: Nm(7), net: Some("N".into()), apertures: None }
}
fn project(pad: Pad) -> Project {
    let mut p = Project::new("Original aperture fixture");
    let part = Part {
        artwork: None,
        assembly: None,
        schematic_symbol: None,
        schematic_package: None,
        pads: Some(Pads::try_from(vec![pad]).unwrap()),
        id: ObjectId::new(),
        designator: "U1".into(),
        value: "Original".into(),
        footprint: "Odd local geometry".into(),
        position: Point::ORIGIN,
        rotation_mdeg: 0,
        side: Side::Top,
    };
    p.board.parts.insert(part.id, part);
    p
}
#[test]
fn legacy_pad_bytes_and_native_minimum_are_preserved_while_explicit_empty_requires14() {
    let legacy = project(pad());
    let text = file::to_string(&legacy);
    assert!(!text.contains("apertures"));
    assert_eq!(file::minimum_version(&legacy), 3);
    assert_eq!(file::to_string(&file::from_str(&text).unwrap()), text);
    let mut explicit = legacy.clone();
    let part = explicit.board.parts.values_mut().next().unwrap();
    part.pads.as_mut().unwrap().0[0].apertures = Some(PadApertures { mask: None, paste: None });
    assert_eq!(file::minimum_version(&explicit), 14);
    let text14 = file::to_string(&explicit);
    assert_eq!(file::from_str(&text14).unwrap(), explicit);
    assert!(file::from_str(&text14.replacen("\"version\": 14", "\"version\": 13", 1)).is_err());
    explicit.board.parts.values_mut().next().unwrap().pads.as_mut().unwrap().0[0].apertures = None;
    assert_eq!(file::to_string(&explicit), text);
}
#[test]
fn library3_requires_real_policy_and_library1or2_refuse_even_null_member() {
    let mut f = Footprint {
        artwork: None,
        name: "Original".into(),
        pads: vec![FootprintPad {
            number: "1".into(),
            position: LocalPosition { x: Nm(1), y: Nm(-3) },
            width: Nm(5),
            height: Nm(7),
            apertures: None,
        }],
    };
    let legacy = viamind_doc::footprint::to_vec(&f).unwrap();
    assert!(!String::from_utf8_lossy(&legacy).contains("apertures"));
    f.pads[0].apertures = Some(PadApertures { mask: None, paste: Some(ApertureRect { width: Nm(3), height: Nm(5) }) });
    let explicit = viamind_doc::footprint::to_vec(&f).unwrap();
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&explicit).unwrap()["version"], 3);
    assert_eq!(viamind_doc::footprint::from_slice(&explicit).unwrap(), f);
    for version in [1, 2] {
        let mut raw: serde_json::Value = serde_json::from_slice(&explicit).unwrap();
        raw["version"] = version.into();
        raw["footprint"]["pads"][0]["apertures"] = serde_json::Value::Null;
        assert!(viamind_doc::footprint::from_slice(&serde_json::to_vec(&raw).unwrap()).unwrap_err().contains("forbid"));
    }
    let mut raw: serde_json::Value = serde_json::from_slice(&legacy).unwrap();
    raw["version"] = 3.into();
    assert!(viamind_doc::footprint::from_slice(&serde_json::to_vec(&raw).unwrap()).unwrap_err().contains("at least one"));
    let duplicate = String::from_utf8(explicit).unwrap().replace("\"mask\": null", "\"mask\": null, \"mask\": null");
    assert!(viamind_doc::footprint::from_slice(duplicate.as_bytes()).unwrap_err().contains("duplicate"));
}
#[test]
fn regular_copies_keep_policies_and_analysis_shell_drops_them_before_copy() {
    let mut pad = pad();
    pad.apertures = Some(PadApertures { mask: None, paste: None });
    let p = project(pad);
    let part = p.board.parts.values().next().unwrap();
    assert_eq!(part.clone_without_assembly().pads, part.pads);
    assert!(part.clone_for_electrical_analysis().pads.unwrap().0[0].apertures.is_none());
    assert!(p.board.parts.values().next().unwrap().pads.as_ref().unwrap().0[0].apertures.is_some());
}
