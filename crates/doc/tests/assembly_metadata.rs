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
    };
    assert!(validate_metadata(std::iter::repeat_n(&full, 1024)).is_ok());
    assert!(validate_metadata(std::iter::repeat_n(&full, 1025)).is_err());
    assert!(validate_metadata(std::iter::empty()).is_ok());
}
