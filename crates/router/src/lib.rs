//! Exact grid snapping shared by local PCB route previews and committed commands.
pub mod posture;
pub use posture::{CornerMode, PadPathPlan, Posture, plan_pad_path};
pub mod walkaround;
pub use walkaround::{WalkaroundPlan, WalkaroundWork, plan_walkaround};

use viamind_geom::{Nm, Point};

/// Snap to the nearest origin-anchored grid, with exact ties away from zero.
/// i128 intermediates preserve signed i64 coordinates and detect rounded overflow.
pub fn snap(point: Point, grid: Nm) -> Result<Point, &'static str> {
    if grid.0 <= 0 {
        return Err("grid must be a positive integer number of nanometres");
    }
    let coordinate = |n: i64| {
        let value = i128::from(n);
        let step = i128::from(grid.0);
        let magnitude = value.abs();
        let rounded = ((magnitude + step / 2) / step) * step * value.signum();
        i64::try_from(rounded).map(Nm).map_err(|_| "snapped coordinate overflow")
    };
    Ok(Point::new(coordinate(point.x.0)?, coordinate(point.y.0)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_ties_and_odd_grids_are_exact() {
        for (input, expected) in [(0, 0), (49, 0), (50, 100), (51, 100), (-49, 0), (-50, -100), (-151, -200)] {
            assert_eq!(snap(Point::new(Nm(input), Nm(-input)), Nm(100)).unwrap(), Point::new(Nm(expected), Nm(-expected)));
        }
        assert_eq!(snap(Point::new(Nm(1), Nm(2)), Nm(3)).unwrap(), Point::new(Nm(0), Nm(3)));
        assert_eq!(snap(Point::new(Nm(-1), Nm(-2)), Nm(3)).unwrap(), Point::new(Nm(0), Nm(-3)));
        assert!(snap(Point::ORIGIN, Nm(0)).is_err());
        assert!(snap(Point::ORIGIN, Nm(-1)).is_err());
    }
    #[test]
    fn extreme_coordinates_never_wrap() {
        let p = Point::new(Nm(i64::MIN), Nm(i64::MAX));
        assert_eq!(snap(p, Nm(1)).unwrap(), p);
        assert!(snap(p, Nm(2)).is_err());
        assert_eq!(snap(Point::new(Nm(i64::MAX), Nm(-i64::MAX)), Nm(i64::MAX)).unwrap(), Point::new(Nm(i64::MAX), Nm(-i64::MAX)));
    }
}
