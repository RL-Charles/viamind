use viamind_doc::{NamedNetMinima, Project, file, named_net_rule_budget};
use viamind_geom::Nm;
fn minima() -> NamedNetMinima {
    NamedNetMinima { minimum_copper_clearance: Nm(750_000), minimum_track_width: Nm(300_000) }
}
#[test]
fn native_empty_bytes_omit_policy_and_nonempty_require_version_eight() {
    let mut project = Project::new("Rules");
    let legacy = file::to_string(&project);
    assert!(!legacy.contains("net_rules"));
    assert!(legacy.contains("\"version\": 1"));
    project.board.net_rules.insert("A".into(), minima());
    let saved = file::to_string(&project);
    assert!(saved.contains("\"version\": 8"));
    assert_eq!(file::from_str(&saved).unwrap(), project);
    assert!(file::from_str(&saved.replace("\"version\": 8", "\"version\": 7")).is_err());
    project.board.net_rules.clear();
    assert_eq!(file::to_string(&project), legacy);
}
#[test]
fn native_original_tokens_reject_duplicate_rules_and_preserve_header_precedence() {
    let project = Project::new("Rules");
    let original = file::to_string(&project);
    let duplicate = original.replace("\"board\": {", "\"board\": {\"net_rules\": {\"A\": {\"minimum_copper_clearance\":1,\"minimum_track_width\":1},\"A\": {\"minimum_copper_clearance\":2,\"minimum_track_width\":2}},");
    assert!(file::from_str(&duplicate).unwrap_err().to_string().contains("duplicate exact"));
    let bad_format = duplicate.replace("\"format\": \"viamind\"", "\"format\": \"other\"");
    assert!(matches!(file::from_str(&bad_format), Err(file::FileError::WrongFormat(_))));
    let future = duplicate.replace("\"version\": 1", "\"version\": 999");
    assert!(matches!(file::from_str(&future), Err(file::FileError::UnsupportedVersion(999))));
}
#[test]
fn borrowed_and_native_bounds_reject_names_rows_and_scalars() {
    let mut project = Project::new("Rules");
    project.board.net_rules.insert("A".repeat(257), minima());
    assert!(named_net_rule_budget(&project.board.net_rules).is_err());
    assert!(file::from_str(&file::to_string(&project)).is_err());
    project.board.net_rules.clear();
    for index in 0..129 {
        project.board.net_rules.insert(format!("N{index}"), minima());
    }
    assert!(named_net_rule_budget(&project.board.net_rules).is_err());
    assert!(file::from_str(&file::to_string(&project)).is_err());
    project.board.net_rules.clear();
    project.board.net_rules.insert("A".into(), NamedNetMinima { minimum_track_width: Nm(0), ..minima() });
    assert!(named_net_rule_budget(&project.board.net_rules).is_err());
}
