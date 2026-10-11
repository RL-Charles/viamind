//! Explicit generic centroid and orientation-marker export, in a common top view.
use crate::{
    assembly::row_limited,
    assembly_templates::{populated, validate_reference},
    snapshot::invalid,
};
use std::{collections::BTreeSet, io};
use viamind_doc::{Part, Side};

const MAX_COORD: i64 = 99_999_999_999;

/// Borrowed validation precedes snapshot and reference copies.
pub(crate) fn validate<'a>(parts: impl Iterator<Item = &'a Part> + Clone) -> io::Result<()> {
    // Validate scalar data before reference-key allocation.
    for part in parts.clone().filter(|part| populated(part)) {
        coordinates(part)?;
    }
    let mut references = BTreeSet::new();
    for part in parts.filter(|part| populated(part)) {
        validate_reference(part, &mut references, "assembly_placement")?;
    }
    Ok(())
}

fn coordinates(part: &Part) -> io::Result<(i64, i64, i32)> {
    let fail = |reason| invalid(format!("assembly_placement: component {:?}: {reason}", part.designator));
    let metadata = part.assembly.as_ref().ok_or_else(|| fail("explicit mounting and placement calibration are required"))?;
    if metadata.mounting.is_none() {
        return Err(fail("explicit mounting is required"));
    }
    let placement = metadata.placement.ok_or_else(|| fail("explicit placement calibration is required"))?;
    placement.validate().map_err(fail)?;
    if !(0..360_000).contains(&part.rotation_mdeg)
        || [part.position.x.0, part.position.y.0].iter().any(|value| !(-MAX_COORD..=MAX_COORD).contains(value))
    {
        return Err(fail("component origin must fit ±99999.999999 mm and rotation 0..359999 millidegrees"));
    }
    let (x, y) = if part.rotation_mdeg % 90_000 == 0 {
        let world = part.local_to_world_nm_exact(placement.centroid_local).map_err(fail)?;
        (world.x.0, world.y.0)
    } else {
        let (x, y) = part.local_to_world_mm(placement.centroid_local.x.to_mm(), placement.centroid_local.y.to_mm());
        let rounded = |value: f64| -> io::Result<i64> {
            let nm = (value * 1_000_000.0).round();
            if !nm.is_finite() || nm < -MAX_COORD as f64 || nm > MAX_COORD as f64 {
                return Err(fail("transformed centroid must fit ±99999.999999 mm"));
            }
            Ok(nm as i64)
        };
        (rounded(x)?, rounded(y)?)
    };
    if [x, y].iter().any(|value| !(-MAX_COORD..=MAX_COORD).contains(value)) {
        return Err(fail("transformed centroid must fit ±99999.999999 mm"));
    }
    let rotation = match part.side {
        Side::Top => part.rotation_mdeg + placement.zero_axis_mdeg,
        Side::Bottom => part.rotation_mdeg + 180_000 - placement.zero_axis_mdeg,
    }
    .rem_euclid(360_000);
    Ok((x, -y, rotation))
}

pub(crate) fn render(parts: &[&Part], limit: usize) -> io::Result<String> {
    let mut output = String::new();
    row_limited(
        &mut output,
        &["Designator", "Component ID", "Value", "Footprint", "Mounting", "X (mm, global)", "Y (mm, up)", "Rotation (deg CCW, top view)", "Side"],
        limit,
    )?;
    for part in parts.iter().copied().filter(|part| populated(part)) {
        let (x, y, angle) = coordinates(part)?;
        let mounting = part.assembly.as_ref().and_then(|metadata| metadata.mounting).expect("validated mounting");
        row_limited(
            &mut output,
            &[
                &part.designator,
                &part.id.to_string(),
                &part.value,
                &part.footprint,
                mounting.name(),
                &super::assembly::fixed(x, 6),
                &super::assembly::fixed(y, 6),
                &super::assembly::fixed(i64::from(angle), 3),
                match part.side {
                    Side::Top => "top",
                    Side::Bottom => "bottom",
                },
            ],
            limit,
        )?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calibrated_header_limit_is_charged_before_append() {
        let expected = render(&[], 32 * 1024 * 1024).unwrap();
        assert!(render(&[], expected.len() - 1).is_err());
        assert_eq!(render(&[], expected.len()).unwrap(), expected);
    }
}
