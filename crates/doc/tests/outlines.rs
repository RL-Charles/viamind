//! Native outline validation and schema compatibility.
use viamind_doc::{BoardOutline, Project, file};
use viamind_geom::{Point, Polygon};
fn ring(points: &[(f64, f64)]) -> Polygon {
    Polygon::new(points.iter().map(|&(x, y)| Point::from_mm(x, y)).collect())
}
fn rectangle(x: f64, y: f64, w: f64, h: f64) -> Polygon {
    ring(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
}
#[test]
fn native_outlines_round_trip_and_legacy_documents_keep_exact_bytes() {
    let mut project = Project::new("Outline");
    let legacy = file::to_string(&project);
    assert!(legacy.contains("\"version\": 1"));
    assert_eq!(file::to_string(&file::from_str(&legacy).unwrap()), legacy);
    project.board.outline = Some(BoardOutline::new(rectangle(0.0, 0.0, 100.0, 80.0), vec![rectangle(20.0, 20.0, 10.0, 10.0)]).unwrap());
    let text = file::to_string(&project);
    assert!(text.contains("\"version\": 2"));
    assert_eq!(file::from_str(&text).unwrap(), project);
    assert_eq!(file::to_string(&file::from_str(&text).unwrap()), text);
    assert!(file::from_str(&text.replace("\"version\": 2", "\"version\": 1")).unwrap_err().to_string().contains("requires native project version 2"));
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["project"]["board"]["outline"]["outer"][1] = value["project"]["board"]["outline"]["outer"][0].clone();
    assert!(file::from_str(&value.to_string()).is_err(), "native deserialization validates geometry");
}
#[test]
fn rejects_invalid_rings_and_cutout_topology_in_either_winding() {
    let outer = rectangle(0.0, 0.0, 100.0, 80.0);
    for bad in [
        ring(&[(0.0, 0.0), (1.0, 1.0)]),
        ring(&[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)]),
        ring(&[(0.0, 0.0), (10.0, 10.0), (0.0, 10.0), (10.0, 0.0)]),
        ring(&[(0.0, 0.0), (10.0, 0.0), (5.0, 0.0), (5.0, 10.0), (0.0, 10.0)]),
        rectangle(10001.0, 0.0, 2.0, 2.0),
    ] {
        assert!(BoardOutline::new(bad, vec![]).is_err());
    }
    for holes in [
        vec![rectangle(-1.0, 20.0, 5.0, 5.0)],
        vec![rectangle(0.0, 20.0, 5.0, 5.0)],
        vec![rectangle(20.0, 20.0, 20.0, 20.0), rectangle(30.0, 30.0, 20.0, 20.0)],
        vec![rectangle(20.0, 20.0, 20.0, 20.0), rectangle(25.0, 25.0, 5.0, 5.0)],
    ] {
        assert!(BoardOutline::new(outer.clone(), holes).is_err());
    }
    let reversed = Polygon::new(outer.points.into_iter().rev().collect());
    assert!(BoardOutline::new(reversed, vec![rectangle(20.0, 20.0, 5.0, 5.0), rectangle(40.0, 20.0, 5.0, 5.0)]).is_ok());
    let too_many = Polygon::new(vec![Point::ORIGIN; 2049]);
    assert!(BoardOutline::new(too_many, vec![]).is_err());
}
