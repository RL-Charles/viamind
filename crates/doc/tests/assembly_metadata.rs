use viamind_doc::{AssemblyMetadata, assembly::validate_metadata};
#[test]
fn source_fields_and_global_limits_use_utf8_bytes_and_borrowed_records() {
    assert!(AssemblyMetadata::validate_field(&"é".repeat(256)).is_ok());
    for s in [String::new(), " ".into(), "a\n".into(), "x ".into(), "\0".into(), "é".repeat(257)] {
        assert!(AssemblyMetadata::validate_field(&s).is_err());
    }
    let dnp = AssemblyMetadata { populate: false, ..Default::default() };
    assert!(validate_metadata(std::iter::repeat_n(&dnp, 100000)).is_ok());
    assert!(validate_metadata(std::iter::repeat_n(&dnp, 100001)).is_err());
    let full = AssemblyMetadata {
        manufacturer: Some("a".repeat(512)),
        mpn: Some("b".repeat(512)),
        supplier: Some("c".repeat(512)),
        supplier_sku: Some("d".repeat(512)),
        populate: true,
        ..Default::default()
    };
    assert!(validate_metadata(std::iter::repeat_n(&full, 1024)).is_ok());
    assert!(validate_metadata(std::iter::repeat_n(&full, 1025)).is_err());
    assert!(validate_metadata(std::iter::empty()).is_ok());
}

#[test]
fn optional_calibration_is_strict_bounded_and_absent_bytes_stay_identical() {
    use serde_json::json;
    use viamind_doc::{AssemblyPlacement, MountingTechnology};
    use viamind_geom::{Nm, Point};
    let old = r#"{"manufacturer":null,"mpn":null,"supplier":null,"supplier_sku":null,"populate":true}"#;
    assert_eq!(serde_json::to_string(&AssemblyMetadata::default()).unwrap(), old);
    assert_eq!(serde_json::from_str::<AssemblyMetadata>(old).unwrap(), AssemblyMetadata::default());
    let placement = AssemblyPlacement { centroid_local: Point::new(Nm(-10_000_000_000), Nm(10_000_000_000)), zero_axis_mdeg: 359_999 };
    let mut metadata = AssemblyMetadata { placement: Some(placement), ..Default::default() };
    assert!(!metadata.is_default());
    assert!(metadata.validate().is_ok());
    assert_eq!(metadata.minimum_version(), 9);
    metadata.placement = None;
    metadata.mounting = Some(MountingTechnology::ThroughHole);
    assert!(!metadata.is_default());
    assert_eq!(metadata.minimum_version(), 9);
    for (x, angle) in [(i64::MIN, 0), (i64::MAX, 0), (-10_000_000_001, 0), (10_000_000_001, 0), (0, -1), (0, 360_000)] {
        let invalid = AssemblyPlacement { centroid_local: Point::new(Nm(x), Nm(0)), zero_axis_mdeg: angle };
        assert!(invalid.validate().is_err());
    }
    for invalid in [
        json!({"mounting":"automatic"}),
        json!({"placement":{}}),
        json!({"placement":{"centroid_local":{"x":0,"y":0}}}),
        json!({"placement":{"centroid_local":{"x":0,"y":0,"z":0},"zero_axis_mdeg":0}}),
        json!({"placement":{"centroid_local":{"x":0.5,"y":0},"zero_axis_mdeg":0}}),
        json!({"placement":{"centroid_local":{"x":0,"y":0},"zero_axis_mdeg":0.5}}),
        json!({"placement":{"centroid_local":{"x":0,"y":0},"zero_axis_mdeg":0,"inferred":true}}),
    ] {
        assert!(serde_json::from_value::<AssemblyMetadata>(invalid.clone()).is_err(), "{invalid}");
    }
}

#[test]
fn mixed_named_net_policy_and_calibration_use_maximum_feature_version() {
    use viamind_doc::{MountingTechnology, NamedNetMinima, file};
    use viamind_geom::Nm;
    let mut project = file::from_str(include_str!("../../../fixtures/fabrication/asymmetric.viamind.json")).unwrap();
    let id = *project.board.parts.keys().next().unwrap();
    project.board.net_rules.insert(
        "GND".into(),
        NamedNetMinima { minimum_copper_clearance: Nm(0), minimum_track_width: Nm(250_000), minimum_via_drill: Nm(0), minimum_annular_width: Nm(0) },
    );
    assert_eq!(file::minimum_version(&project), 8);
    project.board.parts.get_mut(&id).unwrap().assembly =
        Some(Box::new(AssemblyMetadata { mpn: Some("explicit".into()), mounting: Some(MountingTechnology::Hybrid), ..Default::default() }));
    assert_eq!(file::minimum_version(&project), 9);
    assert_eq!(file::from_str(&file::to_string(&project)).unwrap(), project);
    let mut old: serde_json::Value = serde_json::from_str(&file::to_string(&project)).unwrap();
    old["version"] = serde_json::json!(8);
    assert!(file::from_str(&old.to_string()).is_err());
    project.board.parts.get_mut(&id).unwrap().assembly.as_mut().unwrap().mounting = None;
    assert_eq!(file::minimum_version(&project), 8);
    project.board.net_rules.clear();
    assert_eq!(file::minimum_version(&project), 7);
    project.board.parts.get_mut(&id).unwrap().assembly = None;
    assert_eq!(file::minimum_version(&project), 4);
}
