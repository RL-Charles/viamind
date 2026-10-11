//! Property tests for unit conversions and exact integer moves.

use proptest::prelude::*;
use viamind_geom::{NM_PER_INCH, NM_PER_MIL, NM_PER_MM, Nm, Point, Vector};

/// About ±10 m, comfortably larger than any board.
const RANGE_NM: i64 = 10_000_000_000;

/// Largest error, in the unit itself, of a correctly rounded conversion: half a nanometre.
fn half_nm(nm_per_unit: i64) -> f64 {
    0.5 / nm_per_unit as f64
}

fn coord() -> impl Strategy<Value = Nm> {
    (-RANGE_NM..=RANGE_NM).prop_map(Nm)
}

/// Moves as a user would enter them: millimetre offsets up to ±100 mm.
fn mm_move() -> impl Strategy<Value = Vector> {
    (-100.0f64..100.0, -100.0f64..100.0).prop_map(|(x, y)| Vector::from_mm(x, y))
}

proptest! {
    #[test]
    fn nm_round_trips_through_mm_mil_inch(n in coord()) {
        prop_assert!((Nm::from_mm(n.to_mm()) - n).0.abs() <= 1);
        prop_assert!((Nm::from_mil(n.to_mil()) - n).0.abs() <= 1);
        prop_assert!((Nm::from_inch(n.to_inch()) - n).0.abs() <= 1);
    }

    #[test]
    fn mm_round_trips_within_1nm(mm in -10_000.0f64..10_000.0) {
        let err = (Nm::from_mm(mm).to_mm() - mm).abs();
        prop_assert!(err <= half_nm(NM_PER_MM) * 1.000_001, "error {err} mm");
    }

    #[test]
    fn mil_round_trips_within_1nm(mil in -400_000.0f64..400_000.0) {
        let err = (Nm::from_mil(mil).to_mil() - mil).abs();
        prop_assert!(err <= half_nm(NM_PER_MIL) * 1.000_001, "error {err} mil");
    }

    #[test]
    fn inch_round_trips_within_1nm(inch in -400.0f64..400.0) {
        let err = (Nm::from_inch(inch).to_inch() - inch).abs();
        prop_assert!(err <= half_nm(NM_PER_INCH) * 1.000_001, "error {err} in");
    }

    #[test]
    fn thousand_moves_then_reversed_have_zero_drift(
        start in (coord(), coord()).prop_map(|(x, y)| Point::new(x, y)),
        moves in prop::collection::vec(mm_move(), 1_000),
    ) {
        let mut p = start;
        for &v in &moves {
            p += v;
        }
        let total: Vector = moves.iter().fold(Vector::ZERO, |acc, &v| acc + v);
        prop_assert_eq!(p - start, total);
        for &v in moves.iter().rev() {
            p -= v;
        }
        prop_assert_eq!(p, start);
    }

    #[test]
    fn serde_json_round_trip(
        n in coord(),
        p in (coord(), coord()).prop_map(|(x, y)| Point::new(x, y)),
        v in (coord(), coord()).prop_map(|(x, y)| Vector::new(x, y)),
    ) {
        prop_assert_eq!(serde_json::from_str::<Nm>(&serde_json::to_string(&n).unwrap()).unwrap(), n);
        prop_assert_eq!(serde_json::from_str::<Point>(&serde_json::to_string(&p).unwrap()).unwrap(), p);
        prop_assert_eq!(serde_json::from_str::<Vector>(&serde_json::to_string(&v).unwrap()).unwrap(), v);
    }
}
