use viamind_doc::Project;
use viamind_engine::{Command, CommandCall, EngineHandle, NamedNetRuleRow, Origin, SetNamedNetRules};
use viamind_geom::Nm;
fn edit(e: &EngineHandle, width: i64) -> SetNamedNetRules {
    let s = e.session_state();
    SetNamedNetRules {
        expected_project: s.project.id,
        expected_cursor: s.cursor,
        rules: vec![NamedNetRuleRow { net: "A".into(), minimum_copper_clearance: Nm(600_000), minimum_track_width: Nm(width) }],
    }
}
#[test]
fn typed_dynamic_policy_has_one_undo_redo_noop_and_stale_atomic_refusal() {
    for typed in [true, false] {
        let e = EngineHandle::new(Project::new("Rules"));
        let before = e.session_state();
        let input = edit(&e, 300_000);
        if typed {
            e.execute_typed(input.clone(), Origin::User).unwrap();
        } else {
            e.execute(SetNamedNetRules::NAME, serde_json::to_value(&input).unwrap(), Origin::agent("rules")).unwrap();
        }
        let applied = e.session_state();
        assert_eq!(applied.cursor.sequence, before.cursor.sequence + 1);
        assert_eq!(applied.project.board.net_rules.len(), 1);
        assert!(e.execute_typed(input, Origin::User).is_err());
        assert_eq!(e.session_state(), applied);
        e.execute_typed(edit(&e, 300_000), Origin::User).unwrap();
        assert_eq!(e.session_state(), applied);
        let snapshot = e.query_named_net_rules().unwrap();
        assert_eq!(snapshot.source_cursor, applied.cursor);
        assert_eq!(snapshot.document_net_rules, applied.project.board.net_rules);
        e.undo(Origin::User).unwrap();
        assert_eq!(e.session_state().project, before.project);
        e.redo(Origin::User).unwrap();
        assert_eq!(e.session_state().project, applied.project);
    }
}
#[test]
fn duplicate_and_batch_stale_policy_reject_the_entire_edit() {
    let e = EngineHandle::new(Project::new("Rules"));
    let before = e.session_state();
    let mut input = edit(&e, 300_000);
    input.rules.push(input.rules[0].clone());
    assert!(e.execute_typed(input, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
    let mut bad = edit(&e, 300_000);
    bad.expected_cursor.sequence += 1;
    let calls = vec![
        CommandCall { name: SetNamedNetRules::NAME.into(), input: serde_json::to_value(edit(&e, 300_000)).unwrap() },
        CommandCall { name: SetNamedNetRules::NAME.into(), input: serde_json::to_value(bad).unwrap() },
    ];
    assert!(e.execute_batch(calls, Origin::User, None).is_err());
    assert_eq!(e.session_state(), before);
}

#[test]
fn policy_log_version_guards_chain_and_same_project_reopen() {
    let e = EngineHandle::new(Project::new("Policy log"));
    let stale = edit(&e, 300_000);
    e.execute_typed(stale.clone(), Origin::User).unwrap();
    let mut log = Vec::new();
    e.with_engine(|e| e.log().write_jsonl(&mut log)).unwrap();
    assert_eq!(viamind_engine::EventLog::read_jsonl(log.as_slice()).unwrap().replay(), e.read(Clone::clone));
    let text = String::from_utf8(log).unwrap();
    assert!(viamind_engine::EventLog::read_jsonl(text.replace("\"version\":8", "\"version\":7").as_bytes()).is_err());
    let mut lines: Vec<serde_json::Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    lines[1]["deltas"][0]["before"] = lines[1]["deltas"][0]["after"].clone();
    let bad = lines.iter().map(serde_json::Value::to_string).collect::<Vec<_>>().join("\n");
    assert!(viamind_engine::EventLog::read_jsonl(bad.as_bytes()).is_err());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("same.json");
    e.save_project(viamind_engine::SaveProject { path: Some(path.clone()), overwrite: false }).unwrap();
    let current = edit(&e, 400_000);
    e.open_project(viamind_engine::OpenProject { path, discard_dirty: false, expected_cursor: None }, Origin::User).unwrap();
    let before = e.session_state();
    assert_eq!(before.project.id, current.expected_project);
    assert_ne!(before.cursor.stream_id, current.expected_cursor.stream_id);
    assert!(e.execute_typed(current, Origin::User).is_err());
    assert_eq!(e.session_state(), before);
}

#[test]
fn raw_request_row_count_and_name_bounds_refuse_before_row_materialization() {
    let e = EngineHandle::new(Project::new("Bounded request"));
    let input = edit(&e, 1);
    let value = serde_json::to_value(input).unwrap();
    let row = r#"{"net":"N","minimum_copper_clearance":0,"minimum_track_width":1}"#;
    // The 129th row is deliberately not a row at all: count refusal must win before parsing it.
    let rows = format!("[{},null]", vec![row; 128].join(","));
    let raw = format!(r#"{{"expected_project":{},"expected_cursor":{},"rules":{}}}"#, value["expected_project"], value["expected_cursor"], rows);
    assert!(serde_json::from_str::<SetNamedNetRules>(&raw).unwrap_err().to_string().contains("count exceeds 128"));
    let bad_name = raw.replacen("\"net\":\"N\"", &format!("\"net\":\"{}\"", "x".repeat(257)), 1);
    assert!(serde_json::from_str::<SetNamedNetRules>(&bad_name).unwrap_err().to_string().contains("256"));
}

#[test]
fn grouped_rule_edits_share_initial_cursor_coalesce_and_undo_as_one_map() {
    let e = EngineHandle::new(Project::new("Grouped exact rules"));
    let before = e.session_state();
    let first = edit(&e, 300_001);
    let second = edit(&e, 300_002);
    let result = e
        .execute_batch(
            vec![
                CommandCall::new(SetNamedNetRules::NAME, serde_json::to_value(first).unwrap()),
                CommandCall::new(SetNamedNetRules::NAME, serde_json::to_value(second).unwrap()),
            ],
            Origin::User,
            None,
        )
        .unwrap();
    assert_eq!(result.deltas.len(), 1);
    assert_eq!(e.event_cursor().sequence, before.cursor.sequence + 1);
    assert_eq!(e.read(|p| p.board.net_rules["A"].minimum_track_width), Nm(300_002));
    e.undo(Origin::User).unwrap();
    assert_eq!(e.read(Clone::clone), before.project);
    e.redo(Origin::User).unwrap();
    let applied = e.session_state();
    let original = edit(&e, 300_002);
    let changed = edit(&e, 300_003);
    let result = e
        .execute_batch(
            vec![
                CommandCall::new(SetNamedNetRules::NAME, serde_json::to_value(changed).unwrap()),
                CommandCall::new(SetNamedNetRules::NAME, serde_json::to_value(original).unwrap()),
            ],
            Origin::User,
            None,
        )
        .unwrap();
    assert!(result.deltas.is_empty());
    assert_eq!(e.session_state(), applied);
}
