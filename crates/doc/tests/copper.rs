use viamind_doc::{CopperLayer, ObjectId, Project, Track, Via, file};
use viamind_geom::{Nm, Point};

fn project() -> Project {
    let mut p = Project::new("Copper");
    let t = Track {
        id: ObjectId::new(),
        start: Point::new(Nm(-1), Nm(2)),
        end: Point::new(Nm(1_234_567), Nm(-3_456_789)),
        width: Nm(250_001),
        layer: CopperLayer::Bottom,
        net: Some("GND".into()),
    };
    let v = Via { id: ObjectId::new(), position: t.end, diameter: Nm(600_000), drill: Nm(300_000), net: t.net.clone() };
    p.board.tracks.insert(t.id, t);
    p.board.vias.insert(v.id, v);
    p
}
#[test]
fn native_copper_round_trip_identity_geometry_and_legacy_bytes() {
    let p = project();
    let text = file::to_string(&p);
    assert!(text.contains("\"version\": 4"));
    assert_eq!(file::from_str(&text).unwrap(), p);
    assert_eq!(file::to_string(&file::from_str(&text).unwrap()), text);
    assert!(file::from_str(&text.replace("\"version\": 4", "\"version\": 3")).is_err());
    let empty = Project::new("legacy");
    let legacy = file::to_string(&empty);
    assert!(legacy.contains("\"version\": 1"));
    assert!(!legacy.contains("tracks"));
    assert!(!legacy.contains("vias"));
    assert_eq!(file::to_string(&file::from_str(&legacy).unwrap()), legacy);
}
#[test]
fn malformed_native_geometry_keys_layers_nets_and_global_ids_reject() {
    let p = project();
    let t = *p.board.tracks.keys().next().unwrap();
    let v = *p.board.vias.keys().next().unwrap();
    let base: serde_json::Value = serde_json::from_str(&file::to_string(&p)).unwrap();
    let track_key = t.to_string();
    let via_key = v.to_string();
    for (path, value) in [
        (vec!["tracks", track_key.as_str(), "width"], serde_json::json!(0)),
        (vec!["tracks", track_key.as_str(), "layer"], serde_json::json!("inner1")),
        (vec!["tracks", track_key.as_str(), "net"], serde_json::json!(" bad ")),
        (vec!["vias", via_key.as_str(), "drill"], serde_json::json!(600_000)),
        (vec!["vias", via_key.as_str(), "type"], serde_json::json!("blind")),
    ] {
        let mut invalid = base.clone();
        let object = &mut invalid["project"]["board"];
        object[path[0]][path[1]][path[2]] = value;
        assert!(file::from_str(&invalid.to_string()).is_err(), "{invalid}");
    }
    let mut p = p.clone();
    let mut via = p.board.vias.remove(&v).unwrap();
    via.id = t;
    p.board.vias.insert(t, via);
    assert!(file::from_str(&file::to_string(&p)).is_err(), "identity collision across copper kinds");
    let mut p = project();
    let via = p.board.vias.values_mut().next().unwrap();
    via.id = ObjectId::new();
    assert!(file::from_str(&file::to_string(&p)).is_err(), "map key mismatch");
    let mut p = project();
    let track = p.board.tracks.values_mut().next().unwrap();
    track.end = track.start;
    assert!(file::from_str(&file::to_string(&p)).is_err(), "zero length track");
    let mut p = project();
    let track = p.board.tracks.values_mut().next().unwrap();
    track.start.x = Nm(i64::MIN);
    assert!(file::from_str(&file::to_string(&p)).is_err(), "bounded coordinate");
}

#[test]
fn native_copper_count_is_bounded_before_geometry_scan() {
    let mut p = project();
    let template = p.board.vias.values().next().unwrap().clone();
    for _ in 0..viamind_doc::MAX_COPPER_OBJECTS {
        let mut via = template.clone();
        via.id = ObjectId::new();
        p.board.vias.insert(via.id, via);
    }
    let error = file::validate_copper(&p).unwrap_err().to_string();
    assert!(error.contains("at most 100000"), "{error}");
}
