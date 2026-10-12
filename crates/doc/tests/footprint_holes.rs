use viamind_doc::{
    FootprintHole, ObjectId, PartHole, Project, Side, file,
    footprint::{self, Footprint},
};
use viamind_geom::{Nm, Point};

fn project() -> Project {
    file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap()
}
fn hole() -> PartHole {
    PartHole { id: ObjectId::new(), key: "mount-1".into(), center: Point::new(Nm(1_000_003), Nm(-2_000_007)), diameter: Nm(3_000_001) }
}
#[test]
fn absent_null_and_explicit_empty_preserve_distinct_native_intent_and_max_versions() {
    let legacy = project();
    let text = file::to_string(&legacy);
    assert!(!text.contains("\"holes\""));
    let id = *legacy.board.parts.keys().next().unwrap();
    let mut explicit = legacy.clone();
    explicit.board.parts.get_mut(&id).unwrap().holes = Some(vec![]);
    assert_eq!(file::minimum_version(&explicit), 15);
    let saved = file::to_string(&explicit);
    assert_eq!(file::from_str(&saved).unwrap(), explicit);
    assert!(file::from_str(&saved.replacen("\"version\": 15", "\"version\": 14", 1)).is_err());
    let nullable = saved.replace("\"holes\": []", "\"holes\": null");
    assert_eq!(file::to_string(&file::from_str(&nullable).unwrap()), text);
    let part = explicit.board.parts.get_mut(&id).unwrap();
    part.artwork = Some(Box::default());
    part.pads.as_mut().unwrap().0[0].apertures = Some(viamind_doc::PadApertures { mask: None, paste: None });
    assert_eq!(file::minimum_version(&explicit), 15);
    explicit.board.parts.get_mut(&id).unwrap().holes = None;
    assert_eq!(file::minimum_version(&explicit), 14);
    explicit.board.parts.get_mut(&id).unwrap().pads.as_mut().unwrap().0[0].apertures = None;
    assert_eq!(file::minimum_version(&explicit), 13);
    explicit.board.parts.get_mut(&id).unwrap().artwork = None;
    assert_eq!(file::to_string(&explicit), text);
}
#[test]
fn strict_rows_centers_duplicates_and_max_plus_one_poison_are_rejected() {
    let row = serde_json::to_string(&hole()).unwrap();
    for raw in [
        row.replace("\"key\":", "\"key\":\"x\",\"key\":"),
        row.replace("\"x\":", "\"extra\":0,\"x\":"),
        row.replace("\"diameter\":", "\"plated\":false,\"diameter\":"),
        row.replace("\"center\":", "\"center\":null,\"center\":"),
    ] {
        assert!(serde_json::from_str::<PartHole>(&raw).is_err(), "{raw}");
    }
    let mut p = project();
    let part = p.board.parts.values_mut().next().unwrap();
    part.holes = Some(vec![hole()]);
    let raw = serde_json::to_value(&*part).unwrap();
    let mut excess = raw.clone();
    let mut rows = vec![serde_json::to_value(hole()).unwrap(); 256];
    rows.push(serde_json::Value::Null);
    excess["holes"] = rows.into();
    let error = serde_json::from_value::<viamind_doc::Part>(excess).unwrap_err().to_string();
    assert!(error.contains("at most256"), "{error}");
    let mut exactly = raw;
    exactly["holes"] = vec![serde_json::to_value(hole()).unwrap(); 256].into();
    assert!(serde_json::from_value::<viamind_doc::Part>(exactly).is_ok());
}
#[test]
fn local_geometry_keys_ids_and_odd_half_edges_have_checked_boundaries() {
    let mut h = hole();
    h.center.x = Nm(9_998_499_999);
    assert!(h.validate().is_ok());
    h.center.x = Nm(9_998_500_000);
    assert!(h.validate().unwrap_err().contains("extents"));
    for center in [i64::MIN, i64::MAX] {
        h.center.x = Nm(center);
        assert!(h.validate().is_err());
    }
    h.center = Point::ORIGIN;
    for diameter in [0, -1, 100_000_001, i64::MAX] {
        h.diameter = Nm(diameter);
        assert!(h.validate().is_err());
    }
    h.diameter = Nm(100_000_000);
    assert!(h.validate().is_ok());
    for key in ["", " leading", "trailing ", "λ", "slash/key", "two keys"] {
        h.key = key.into();
        assert!(h.validate().is_err());
    }
    h.key = "A_z.-9".repeat(10);
    assert!(h.validate().is_ok());
    h.key.push_str("12345");
    assert!(h.validate().is_err());
    h = hole();
    h.id = "00000000-0000-0000-0000-000000000000".parse().unwrap();
    assert!(h.validate().is_err());
    let a = hole();
    let mut b = a.clone();
    b.id = ObjectId::new();
    assert!(viamind_doc::footprint_holes::validate_part_holes(&[a.clone(), b.clone()]).is_err());
    b.key = "Mount-1".into();
    assert!(viamind_doc::footprint_holes::validate_part_holes(&[a, b]).is_ok());
}
#[test]
fn eight_independent_cardinal_mirror_oracles_and_computed_arbitrary_quantization() {
    // Expected values were independently prepared with integer Python before implementation.
    let cases = [
        (Side::Top, 0, 11_000_014, -22_000_020),
        (Side::Top, 90_000, 8_000_004, -21_000_016),
        (Side::Top, 180_000, 9_000_008, -18_000_006),
        (Side::Top, 270_000, 12_000_018, -19_000_010),
        (Side::Bottom, 0, 9_000_008, -22_000_020),
        (Side::Bottom, 90_000, 8_000_004, -19_000_010),
        (Side::Bottom, 180_000, 11_000_014, -18_000_006),
        (Side::Bottom, 270_000, 12_000_018, -21_000_016),
    ];
    let mut part = project().board.parts.into_values().next().unwrap();
    part.position = Point::new(Nm(10_000_011), Nm(-20_000_013));
    let h = hole();
    for (side, angle, x, y) in cases {
        part.side = side;
        part.rotation_mdeg = angle;
        let world = viamind_doc::resolve_hole(&part, &h).unwrap();
        assert_eq!(world.center, Point::new(Nm(x), Nm(y)));
        assert_eq!(world.diameter, Nm(3_000_001));
    }
    part.side = Side::Bottom;
    part.rotation_mdeg = 37_123;
    let world = viamind_doc::resolve_hole(&part, &h).unwrap();
    let (x, y) = part.local_to_world_mm(h.center.x.to_mm(), h.center.y.to_mm());
    assert!((world.center.x.0 as f64 - x * 1_000_000.0).abs() <= 0.5);
    assert!((world.center.y.0 as f64 - y * 1_000_000.0).abs() <= 0.5);
    part.rotation_mdeg = 0;
    part.position.x = Nm(i64::MAX);
    assert!(viamind_doc::resolve_hole(&part, &h).is_err());
}
#[test]
fn global_collisions_refuse_and_padless_analysis_copies_retain_physical_drills() {
    let mut p = project();
    let id = *p.board.parts.keys().next().unwrap();
    let ids: Vec<_> = std::iter::once(p.id)
        .chain(p.board.parts.keys().copied())
        .chain(p.board.tracks.keys().copied())
        .chain(p.board.vias.keys().copied())
        .chain(p.schematic.sheets.keys().copied())
        .chain(p.board.parts.values().flat_map(|p| p.pads.iter().flat_map(|pads| pads.0.iter().map(|p| p.id))))
        .collect();
    let mut h = hole();
    for duplicate in ids {
        h.id = duplicate;
        p.board.parts.get_mut(&id).unwrap().holes = Some(vec![h.clone()]);
        assert!(file::from_str(&file::to_string(&p)).is_err(), "{duplicate}");
    }
    let part = p.board.parts.get_mut(&id).unwrap();
    part.pads = None;
    part.holes = Some(vec![hole()]);
    let expected = part.holes.clone();
    assert_eq!(part.clone_without_assembly().holes, expected);
    assert_eq!(part.clone_for_electrical_analysis().holes, expected);
    assert_eq!(p.board.clone_for_electrical_analysis().parts[&id].holes, expected);
    file::validate_holes(&p).unwrap();
}
#[test]
fn activated_combined_identity_and_key_budgets_precede_invalid_geometry() {
    let mut p = project();
    let part = p.board.parts.values_mut().next().unwrap();
    part.holes = Some(vec![]);
    let track = p.board.tracks.values().next().unwrap().clone();
    p.board.tracks = (0..100_000).map(|_| (ObjectId::new(), track.clone())).collect();
    let error = file::from_str(&file::to_string(&p)).unwrap_err().to_string();
    assert!(error.contains("combined project identities"), "{error}");
    p.board.tracks.clear();
    let mut template = p.board.parts.values().next().unwrap().clone();
    let mut h = hole();
    h.key = "K".repeat(64);
    h.diameter = Nm(0);
    template.holes = Some(vec![h; 256]);
    p.board.parts = (0..129)
        .map(|_| {
            let mut v = template.clone();
            v.id = ObjectId::new();
            (v.id, v)
        })
        .collect();
    let error = file::validate_holes(&p).unwrap_err().to_string();
    assert!(error.contains("keys exceed2MiB"), "{error}");
}
#[test]
fn library4_presence_strict_rows_and_max_composition_preserve_old_bytes() {
    let mut f = Footprint { holes: None, artwork: None, name: "Mechanical".into(), pads: vec![] };
    let legacy = footprint::to_vec(&f).unwrap();
    for holes in [Some(vec![]), Some(vec![FootprintHole { key: "mount".into(), center: Point::ORIGIN, diameter: Nm(3) }])] {
        f.holes = holes;
        let bytes = footprint::to_vec(&f).unwrap();
        assert_eq!(footprint::from_slice(&bytes).unwrap(), f);
        let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(raw["version"], 4);
        for version in [1, 2, 3] {
            for value in [raw["footprint"]["holes"].clone(), serde_json::Value::Null] {
                let mut wrong = raw.clone();
                wrong["version"] = version.into();
                wrong["footprint"]["holes"] = value;
                assert!(footprint::from_slice(&serde_json::to_vec(&wrong).unwrap()).unwrap_err().contains("forbid"));
            }
        }
    }
    f.holes = None;
    assert_eq!(footprint::to_vec(&f).unwrap(), legacy);
    let mut wrong: serde_json::Value = serde_json::from_slice(&legacy).unwrap();
    wrong["version"] = 4.into();
    assert!(footprint::from_slice(&serde_json::to_vec(&wrong).unwrap()).unwrap_err().contains("requires"));
    f.holes = Some(vec![]);
    f.artwork = Some(Box::default());
    assert_eq!(footprint::from_slice(&footprint::to_vec(&f).unwrap()).unwrap(), f);
}

#[test]
fn physical_identity_admission_rejects_global_collisions_without_choosing_a_pose_policy() {
    let mut p = project();
    let owner = *p.board.parts.keys().next().unwrap();
    let mut h = hole();
    h.id = p.id;
    p.board.parts.get_mut(&owner).unwrap().holes = Some(vec![h.clone()]);
    assert!(viamind_doc::footprint_holes::validate_physical_identities(&p).is_err());
    h.id = ObjectId::new();
    p.board.parts.get_mut(&owner).unwrap().holes = Some(vec![h.clone()]);
    let other = *p.board.parts.keys().find(|id| **id != owner).unwrap();
    p.board.parts.get_mut(&other).unwrap().holes = Some(vec![h]);
    assert!(viamind_doc::footprint_holes::validate_physical_identities(&p).is_err());
    p.board.parts.get_mut(&other).unwrap().holes = None;
    let part = p.board.parts.get_mut(&owner).unwrap();
    part.position.x = Nm(i64::MAX);
    part.holes.as_mut().unwrap()[0].center = Point::new(Nm(10), Nm(10));
    assert!(viamind_doc::resolve_hole(part, &part.holes.as_ref().unwrap()[0]).is_err());
    // Unrouted analysis may report unknown for this pose; identity admission does not resolve it.
    assert!(viamind_doc::footprint_holes::validate_physical_identities(&p).is_ok());
}

#[test]
fn physical_identity_admission_includes_explicit_bus_and_entry_namespace() {
    let buses = file::from_str(include_str!("../../../fixtures/schematic/buses.viamind.json")).unwrap();
    let mut p = project();
    p.schematic.sheets.extend(buses.schematic.sheets);
    let owner = *p.board.parts.keys().next().unwrap();
    let pad = p.board.parts[&owner].pads.as_ref().unwrap().0[0].id;
    let h = hole();
    p.board.parts.get_mut(&owner).unwrap().holes = Some(vec![h.clone()]);
    viamind_doc::footprint_holes::validate_physical_identities(&p).unwrap();
    viamind_doc::schematic::validate_project(&p, None).unwrap();
    let sheet = *p.schematic.sheets.keys().next().unwrap();
    let bus = *p.schematic.sheets[&sheet].content.as_ref().unwrap().buses.keys().next().unwrap();
    // Re-key the explicit bus onto a board part, pad and mechanical hole identity in turn.
    for id in [owner, pad, h.id] {
        let mut q = p.clone();
        let c = q.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
        let mut b = c.buses.remove(&bus).unwrap();
        b.id = id;
        c.buses.insert(id, b);
        assert!(viamind_doc::footprint_holes::validate_physical_identities(&q).unwrap_err().contains("duplicate"), "{id}");
        assert!(viamind_doc::schematic::validate_project(&q, None).is_err(), "{id}");
    }
    // A hole reusing the bus identity is the same global collision from the other side.
    let mut q = p.clone();
    q.board.parts.get_mut(&owner).unwrap().holes.as_mut().unwrap()[0].id = bus;
    assert!(viamind_doc::footprint_holes::validate_physical_identities(&q).unwrap_err().contains("duplicate"));
    assert!(viamind_doc::schematic::validate_project(&q, None).is_err());
    // A bus map key that differs from its identity is refused by the physical validator too.
    let mut q = p.clone();
    let c = q.schematic.sheets.get_mut(&sheet).unwrap().content.as_mut().unwrap();
    let b = c.buses.remove(&bus).unwrap();
    c.buses.insert(ObjectId::new(), b);
    assert!(viamind_doc::footprint_holes::validate_physical_identities(&q).is_err());
}
