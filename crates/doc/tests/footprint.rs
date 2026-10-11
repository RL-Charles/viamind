use serde_json::{Value, json};
use viamind_doc::footprint::{self, Footprint, FootprintPad, LocalPosition};
use viamind_geom::Nm;

fn example() -> Footprint {
    Footprint {
        holes: None,
        artwork: None,
        name: "Precise_SMD".into(),
        pads: vec![FootprintPad {
            number: "A1".into(),
            position: LocalPosition { x: Nm(-800001), y: Nm(123) },
            width: Nm(900003),
            height: Nm(950007),
            apertures: None,
        }],
    }
}

#[test]
fn strict_integer_geometry_roundtrips_without_design_fields() {
    let f = example();
    let bytes = footprint::to_vec(&f).unwrap();
    assert_eq!(footprint::from_slice(&bytes).unwrap(), f);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["footprint"]["pads"][0]["position"]["x"], -800001);
    for path in [vec!["net"], vec!["id"], vec!["position", "unexpected"]] {
        let mut bad = value.clone();
        let mut pad = &mut bad["footprint"]["pads"][0];
        for key in &path[..path.len() - 1] {
            pad = &mut pad[*key];
        }
        pad[path[path.len() - 1]] = json!("should reject");
        assert!(footprint::from_slice(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    for (field, invalid) in [("format", json!("viamind")), ("version", json!(0)), ("version", json!(2)), ("unknown", json!(true))] {
        let mut bad = value.clone();
        bad[field] = invalid;
        assert!(footprint::from_slice(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    for alter in [
        |f: &mut Footprint| f.pads[0].width = Nm(0),
        |f: &mut Footprint| f.pads[0].height = Nm(-1),
        |f: &mut Footprint| f.pads[0].position.x = Nm(i64::MIN),
        |f: &mut Footprint| f.pads[0].number = " A1".into(),
        |f: &mut Footprint| f.name = "x".repeat(129),
        |f: &mut Footprint| f.pads.push(f.pads[0].clone()),
        |f: &mut Footprint| f.pads = vec![f.pads[0].clone(); 1025],
    ] {
        let mut bad = f.clone();
        alter(&mut bad);
        assert!(footprint::to_vec(&bad).is_err());
    }
    assert!(footprint::from_slice(&vec![b' '; footprint::MAX_BYTES + 1]).is_err());
}

#[test]
fn bounded_io_preserves_existing_paths_and_native_project_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reusable.json");
    let f = example();
    footprint::save_new(&f, &path, None).unwrap();
    assert_eq!(footprint::load(&path).unwrap(), f);
    let bytes = std::fs::read(&path).unwrap();
    assert!(footprint::save_new(&f, &path, None).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(footprint::save_new(&f, &path, Some(&path)).unwrap_err().contains("active native"));
    assert!(footprint::save_new(&f, dir.path(), None).is_err());
    let oversized = dir.path().join("oversized.json");
    std::fs::write(&oversized, vec![b' '; footprint::MAX_BYTES + 1]).unwrap();
    assert!(footprint::load(&oversized).is_err());
    #[cfg(unix)]
    {
        let fifo = dir.path().join("pipe.json");
        assert!(std::process::Command::new("mkfifo").arg(&fifo).status().unwrap().success());
        assert!(footprint::load(&fifo).unwrap_err().contains("regular file"));
        let link = dir.path().join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(footprint::save_new(&f, &link, Some(&path)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let dangling = dir.path().join("dangling.json");
        std::os::unix::fs::symlink(dir.path().join("absent"), &dangling).unwrap();
        assert!(footprint::save_new(&f, &dangling, None).is_err());
        assert!(std::fs::symlink_metadata(dangling).unwrap().file_type().is_symlink());
    }
}
