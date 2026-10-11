//! Public parser fixtures independently compare exact doubled-Nm corner sets.
use std::collections::BTreeSet;
use viamind_doc::{footprint_sexpr, pcb_sexpr};
use viamind_geom::Nm;
const PCB: &str = include_str!("../../../fixtures/import/original-cardinal-pads.kicad_pcb");
const MOD: &str = include_str!("../../../fixtures/import/original-cardinal-pads.kicad_mod");
const AT: &str = "(at -0.500003 0.250001 90)";
fn native_corners(x: i64, y: i64, width: i64, height: i64) -> BTreeSet<(i128, i128)> {
    [-1_i128, 1]
        .into_iter()
        .flat_map(|sx| [-1_i128, 1].map(|sy| (2 * i128::from(x) + sx * i128::from(width), 2 * i128::from(y) + sy * i128::from(height))))
        .collect()
}
fn source_rotated_corners(angle: i32) -> BTreeSet<(i128, i128)> {
    // Rotate doubled local corners about the independently expected source center;
    // no width/height swap or parser helper is used in this geometric oracle.
    [-800_001_i128, 800_001]
        .into_iter()
        .flat_map(|dx| {
            [-1_200_003_i128, 1_200_003].map(|dy| {
                let (dx, dy) = match angle {
                    0 => (dx, dy),
                    90 => (-dy, dx),
                    180 => (-dx, -dy),
                    270 => (dy, -dx),
                    _ => unreachable!(),
                };
                (-1_000_006 + dx, 500_002 + dy)
            })
        })
        .collect()
}
#[test]
fn both_public_parsers_preserve_signed_cardinal_odd_nm_geometry_and_exhaustive_reports() {
    for (token, source, canonical) in [
        ("0", 0, 0),
        ("90", 90, 90),
        ("180", 180, 180),
        ("270", 270, 270),
        ("-90", -90, 270),
        ("-180", -180, 180),
        ("-270", -270, 90),
        ("-360", -360, 0),
        ("360", 360, 0),
        ("-0", 0, 0),
        ("90.0000000000", 90, 90),
        ("00090", 90, 90),
    ] {
        let at = format!("(at -0.500003 0.250001 {token})");
        let pcb = pcb_sexpr::parse(&PCB.replacen(AT, &at, 1)).unwrap();
        assert!(pcb.report.can_import(), "{token}: {:?}", pcb.report);
        let part = pcb.project.board.parts.values().next().unwrap();
        let pad = part.pads.as_ref().unwrap().0.iter().find(|p| p.number == "1").unwrap();
        let detached = footprint_sexpr::parse(&MOD.replacen(AT, &at, 1)).unwrap();
        let native = detached.footprint.as_ref().unwrap();
        let local = native.pads.iter().find(|p| p.number == "1").unwrap();
        assert_eq!((pad.position.x, pad.position.y), (Nm(-500_003), Nm(250_001)));
        assert_eq!((local.position.x, local.position.y), (pad.position.x, pad.position.y));
        assert_eq!((local.width, local.height), (pad.width, pad.height));
        assert_eq!(native_corners(pad.position.x.0, pad.position.y.0, pad.width.0, pad.height.0), source_rotated_corners(canonical), "{token}");
        assert_eq!(part.local_to_world_nm_exact(pad.position).unwrap().x, Nm(3_500_004));
        assert_eq!(part.local_to_world_nm_exact(pad.position).unwrap().y, Nm(5_250_012));
        assert_eq!(pad.net.as_deref(), Some("VIN"));
        assert!(part.assembly.is_none(), "pad normalization must not infer sourcing or population metadata");
        assert_eq!(pad.id.to_string(), "30000000-0000-4000-8000-000000000001");
        for report in [&pcb.report, &detached.report] {
            assert!(!report.full_fidelity);
            let reason = format!("source angle {source} degrees, canonical {canonical} degrees");
            assert!(report.items.iter().any(|i| i.token == "at" && i.reason.contains(&reason) && i.reason.contains("center unchanged")));
            assert!(report.items.iter().any(|i| i.token == "size" && i.reason.contains("no rounding")));
            assert!(report.items.iter().all(|i| i.reason.len() < 400));
        }
        assert_eq!(pcb.report, pcb_sexpr::parse(&PCB.replacen(AT, &at, 1)).unwrap().report);
        assert_eq!(detached.report, footprint_sexpr::parse(&MOD.replacen(AT, &at, 1)).unwrap().report);
        assert_eq!(viamind_doc::footprint::from_slice(&viamind_doc::footprint::to_vec(native).unwrap()).unwrap(), *native);
    }
    for source in [PCB, MOD] {
        let missing = source.replacen(AT, "(at -0.500003 0.250001)", 1);
        if source == PCB {
            assert!(pcb_sexpr::parse(&missing).unwrap().report.can_import());
        } else {
            assert!(footprint_sexpr::parse(&missing).unwrap().footprint.is_some());
        }
    }
}
#[test]
fn malformed_noncardinal_fractional_and_out_of_bounds_angles_block_the_whole_result() {
    for token in [
        "45",
        "89",
        "91",
        "-45",
        "361",
        "-361",
        "450",
        "-450",
        "720",
        "90.1",
        "90.000000001",
        "-0.000000001",
        "NaN",
        "inf",
        "9e1",
        "+90",
        "90.",
        ".0",
        "--90",
        "90.0.0",
        "999999999999999999999999999999999999",
    ] {
        let at = format!("(at -0.500003 0.250001 {token})");
        let pcb = pcb_sexpr::parse(&PCB.replacen(AT, &at, 1)).unwrap();
        assert!(!pcb.report.can_import(), "{token}");
        assert!(pcb.project.board.parts.is_empty(), "no partial source component: {token}");
        let detached = footprint_sexpr::parse(&MOD.replacen(AT, &at, 1)).unwrap();
        assert!(detached.footprint.is_none(), "{token}");
        for report in [pcb.report, detached.report] {
            assert!(report.items.iter().any(|i| i.reason.contains("angle") && i.reason.contains("-360..360")), "{token}: {report:?}");
        }
    }
    let too_long = "9".repeat(4097);
    for original in [PCB, MOD] {
        let source = original.replacen(AT, &format!("(at -0.500003 0.250001 {too_long})"), 1);
        if original == PCB {
            assert!(pcb_sexpr::parse(&source).is_err());
        } else {
            assert!(footprint_sexpr::parse(&source).is_err());
        }
    }
    for source in
        [PCB.replace("(at 4.000007 5.000011)", "(at 4.000007 5.000011 90)"), PCB.replacen("(layer \"F.Cu\") (at", "(layer \"B.Cu\") (at", 1)]
    {
        assert!(!pcb_sexpr::parse(&source).unwrap().report.can_import());
    }
    for root in ["(at 0 0 90)", "(at 1 0 0)"] {
        let source = MOD.replacen("(attr smd)", &format!("(attr smd) {root}"), 1);
        assert!(footprint_sexpr::parse(&source).unwrap().footprint.is_none());
    }
    for original in [PCB, MOD] {
        let source = original.replace("(attr smd)", "(attr smd dnp)");
        if original == PCB {
            assert!(!pcb_sexpr::parse(&source).unwrap().report.can_import(), "unmodeled source DNP must still block");
        } else {
            assert!(footprint_sexpr::parse(&source).unwrap().footprint.is_none(), "unmodeled source DNP must still block");
        }
    }
}
