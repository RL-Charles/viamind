//! Save/load round trips of the native project file.

use std::path::PathBuf;

use proptest::prelude::*;
use viamind_doc::file::{self, FileError};
use viamind_doc::{ObjectId, Part, Project, Sheet, Side};
use viamind_geom::{Nm, Point};

fn id(n: u128) -> ObjectId {
    ObjectId::from_uuid(uuid::Uuid::from_u128(n))
}

/// The exact bytes of a small project, pinning the format.
#[test]
fn golden_file() {
    let mut project = Project { id: id(1), name: "golden".into(), ..Project::new("") };
    project.schematic.sheets.insert(id(2), Sheet { id: id(2), name: "Main".into() });
    let part = Part {
        id: id(3),
        designator: "R1".into(),
        value: "10k".into(),
        footprint: "R_0603".into(),
        position: Point::new(Nm(1_270_000), Nm(-500_000)),
        rotation_mdeg: 90_000,
        side: Side::Bottom,
    };
    project.board.parts.insert(part.id, part);

    let expected = r#"{
  "format": "viamind",
  "version": 1,
  "project": {
    "id": "00000000-0000-0000-0000-000000000001",
    "name": "golden",
    "schematic": {
      "sheets": {
        "00000000-0000-0000-0000-000000000002": {
          "id": "00000000-0000-0000-0000-000000000002",
          "name": "Main"
        }
      }
    },
    "board": {
      "parts": {
        "00000000-0000-0000-0000-000000000003": {
          "id": "00000000-0000-0000-0000-000000000003",
          "designator": "R1",
          "value": "10k",
          "footprint": "R_0603",
          "position": {
            "x": 1270000,
            "y": -500000
          },
          "rotation_mdeg": 90000,
          "side": "bottom"
        }
      }
    }
  }
}
"#;
    assert_eq!(file::to_string(&project), expected);
    assert_eq!(file::from_str(expected).unwrap(), project);
}

/// A unique path in the system temp directory, removed on drop.
struct TempPath(PathBuf);

impl TempPath {
    fn new(name: &str) -> TempPath {
        TempPath(std::env::temp_dir().join(format!("viamind-{}-{name}.{}", ObjectId::new(), file::EXTENSION)))
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn save_load_and_resave_on_disk() {
    let mut project = Project::new("disk");
    for i in 0..50 {
        let part = Part {
            id: ObjectId::new(),
            designator: format!("C{i}"),
            value: "100nF".into(),
            footprint: "C_0402".into(),
            position: Point::from_mm(f64::from(i), 2.0),
            rotation_mdeg: 0,
            side: Side::Top,
        };
        project.board.parts.insert(part.id, part);
    }
    let path = TempPath::new("disk");
    file::save(&project, &path.0).unwrap();
    let first = std::fs::read(&path.0).unwrap();
    let loaded = file::load(&path.0).unwrap();
    assert_eq!(loaded, project, "save then load yields an equal model");
    file::save(&loaded, &path.0).unwrap();
    assert_eq!(std::fs::read(&path.0).unwrap(), first, "re-saving an unchanged project is byte-identical");
    assert!(first.ends_with(b"}\n") && !first.contains(&b'\r'));
}

#[test]
fn rejects_bad_files_with_clear_errors() {
    let good = file::to_string(&Project::new("p"));

    let err = file::from_str(&good.replace("\"viamind\"", "\"other\"")).unwrap_err();
    assert!(matches!(err, FileError::WrongFormat(ref f) if f == "other"), "{err}");

    let err = file::from_str(&good.replace("\"version\": 1", "\"version\": 99")).unwrap_err();
    assert_eq!(err.to_string(), "project file version 99 is newer than this build supports (version 2)");

    let err = file::from_str(&good[..good.len() - 4]).unwrap_err();
    assert!(err.to_string().contains("line"), "parse errors carry a position: {err}");

    let mut project = Project::new("p");
    let sheet = Sheet::new("Main");
    project.schematic.sheets.insert(sheet.id, sheet.clone());
    let text = file::to_string(&project).replacen(&format!("\"{}\": {{", sheet.id), &format!("\"{}\": {{", id(7)), 1);
    let err = file::from_str(&text).unwrap_err();
    assert!(matches!(err, FileError::KeyMismatch { object: "sheet", .. }), "{err}");

    let err = file::load(&TempPath::new("missing").0).unwrap_err();
    assert!(matches!(err, FileError::Io { .. }));
}

fn arb_part() -> impl Strategy<Value = Part> {
    (any::<u128>(), "[A-Z]{1,3}[0-9]{1,4}", "\\PC{0,12}", "[A-Za-z0-9_.-]{1,20}", any::<i64>(), any::<i64>(), 0..360_000i32, any::<bool>()).prop_map(
        |(n, designator, value, footprint, x, y, rotation_mdeg, bottom)| Part {
            id: id(n),
            designator,
            value,
            footprint,
            position: Point::new(Nm(x), Nm(y)),
            rotation_mdeg,
            side: if bottom { Side::Bottom } else { Side::Top },
        },
    )
}

fn arb_project() -> impl Strategy<Value = Project> {
    ("\\PC{0,20}", prop::collection::vec(("\\PC{0,10}", any::<u128>()), 0..5), prop::collection::vec(arb_part(), 0..30)).prop_map(
        |(name, sheets, parts)| {
            let mut project = Project::new(name);
            for (name, n) in sheets {
                project.schematic.sheets.insert(id(n), Sheet { id: id(n), name });
            }
            for part in parts {
                project.board.parts.insert(part.id, part);
            }
            project
        },
    )
}

proptest! {
    /// Arbitrary projects (any unicode text, full i64 coordinates) survive save
    /// then load unchanged, and re-saving gives the same bytes.
    #[test]
    fn round_trip(project in arb_project()) {
        let text = file::to_string(&project);
        let loaded = file::from_str(&text).unwrap();
        prop_assert_eq!(&loaded, &project);
        prop_assert_eq!(file::to_string(&loaded), text);
    }
}
