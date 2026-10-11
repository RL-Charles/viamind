//! Real CLI invocations, with Python's independent CSV reader checking interchange.
use std::{path::Path, process::Command};

use serde_json::{Value, json};
use viamind_doc::{Project, file};
use viamind_engine::{EngineHandle, Origin};

fn parse_csv(path: &Path) -> Value {
    let result = Command::new("python3")
        .args(["-c", "import csv,json,sys; print(json.dumps(list(csv.reader(open(sys.argv[1], encoding='utf-8', newline='')))))"])
        .arg(path)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    serde_json::from_slice(&result.stdout).unwrap()
}

#[test]
fn cli_exports_lossless_tables_without_display_and_requires_explicit_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let native = directory.path().join("source project.json");
    let output = directory.path().join("assembly file.csv");
    let engine = EngineHandle::new(Project::new("CSV example"));
    let value = "10k, \"precision\"\r\nµΩ";
    for (designator, side) in [("R2", "bottom"), ("R1", "top")] {
        engine
            .execute(
                "add_part",
                json!({"designator":designator,"value":value,"footprint":"0603","x_mm":1.234567,"y_mm":-0.000001,"rotation_deg":-0.001,"side":side}),
                Origin::User,
            )
            .unwrap();
    }
    let id = engine.read(|p| p.board.part_by_designator("R1").unwrap().id);
    engine
        .execute("set_part_assembly", json!({"id":id,"assembly":{"manufacturer":"µ, \"Example\"","mpn":"USER-INPUT","populate":false}}), Origin::User)
        .unwrap();
    engine.read(|project| file::save(project, &native)).unwrap();
    let native_bytes = std::fs::read(&native).unwrap();
    let export = |kind: &str, path: &Path, overwrite: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_viamind"));
        command
            .args(["--export-csv", kind, "--open"])
            .arg(&native)
            .arg("--output")
            .arg(path)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env("VIAMIND_MCP_PORT", "not-used");
        if overwrite {
            command.arg("--overwrite");
        }
        command.output().unwrap()
    };
    let result = export("placement", &output, false);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert!(result.stdout.is_empty());
    let records = parse_csv(&output);
    assert_eq!(records.as_array().unwrap().len(), 3);
    assert_eq!(records[1][0], "R1");
    assert_eq!(records[1][2], value);
    assert_eq!(records[1][3], "0603");
    assert_eq!(records[1][4], "1.234567");
    assert_eq!(records[1][5], "-0.000001");
    assert_eq!(records[1][6], "359.999");
    assert_eq!(records[1][7], "Top");
    assert_eq!(records[2][7], "Bottom");
    let placement_bytes = std::fs::read(&output).unwrap();
    let sourcing = directory.path().join("sourcing.csv");
    let result = export("sourcing_bom", &sourcing, false);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let sourcing_rows = parse_csv(&sourcing);
    assert_eq!(sourcing_rows[0][9], "Population");
    assert!(sourcing_rows.as_array().unwrap().iter().any(|r| r[5] == "µ, \"Example\"" && r[9] == "Do not populate"));
    assert!(!export("bom", &output, false).status.success());
    assert_eq!(std::fs::read(&output).unwrap(), placement_bytes);
    assert!(export("bom", &output, true).status.success());
    let records = parse_csv(&output);
    assert_eq!(records.as_array().unwrap().len(), 2);
    let references: Vec<String> = serde_json::from_str(records[1][0].as_str().unwrap()).unwrap();
    assert_eq!(references, ["R1", "R2"]);
    assert_eq!(records[1][2], "2");
    assert_eq!(records[1][3], value);
    assert_eq!(records[1][4], "0603");
    let ids: Vec<_> = records[1][1].as_str().unwrap().split_whitespace().collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], engine.read(|project| project.board.part_by_designator("R1").unwrap().id.to_string()));
    assert!(!export("bom", &native, true).status.success());
    assert_eq!(std::fs::read(&native).unwrap(), native_bytes);
    assert!(!export("bom", &directory.path().join("missing/destination.csv"), false).status.success());
    for args in [
        vec!["--export-csv"],
        vec!["--export-csv", "bad"],
        vec!["--export-csv", "bom"],
        vec!["--output", "out.csv"],
        vec!["--overwrite"],
        vec!["--headless", "--export-csv", "bom", "--open", "in.json", "--output", "out.csv"],
    ] {
        assert!(
            !Command::new(env!("CARGO_BIN_EXE_viamind"))
                .args(args)
                .env_remove("DISPLAY")
                .env_remove("WAYLAND_DISPLAY")
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
