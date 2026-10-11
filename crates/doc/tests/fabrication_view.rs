use viamind_doc::fabrication_view::{self as view, FabricationGeometry, FabricationLayer as L, FabricationPrimitive as P, FabricationReadBudget};
use viamind_geom::{Nm, Point};
fn header(function: &str, polarity: &str) -> String {
    format!("%FSLAX56Y56*%\n%MOMM*%\n%TF.Part,Single*%\n%TF.FileFunction,{function}*%\n%TF.FilePolarity,{polarity}*%\n%LPD*%\n%LMN*%\n%LS1*%\nG01*\n")
}
fn parse(text: &str) -> Result<FabricationGeometry, String> {
    view::parse_gerber(text, &mut FabricationReadBudget::default())
}
#[test]
fn original_odd_signed_coordinates_rectangles_and_linear_strokes_preserve_native_frame() {
    let text = header("Copper,L2,Bot", "Positive")
        + "%ADD10R,2.000001X1.000003*%\n%ADD11C,0.250001*%\nD10*\n%LR90.001*%\nX-1234567Y2000003D03*\nD11*\n%LR0*%\nX123Y-129D02*\nX9000001Y-3000003D01*\nM02*\n";
    let geometry = parse(&text).unwrap();
    assert_eq!(geometry.layer, L::BottomCopper);
    assert_eq!(
        geometry.primitives,
        vec![
            P::Rectangle { center: Point::new(Nm(-1234567), Nm(-2000003)), width: Nm(2000001), height: Nm(1000003), rotation_mdeg: 90001 },
            P::Segment { start: Point::new(Nm(123), Nm(129)), end: Point::new(Nm(9000001), Nm(3000003)), width: Nm(250001) },
        ]
    );
    assert!(!geometry.negative_file_polarity);
    assert!(geometry.unverified_common_origin);
}
#[test]
fn mask_openings_profile_zero_centerlines_and_xnc_exact_hits_are_distinct() {
    let mask = header("Soldermask,Top", "Negative") + "%ADD10C,0.600001*%D10*X1000001Y-2000003D03*M02*";
    assert!(parse(&mask).unwrap().negative_file_polarity);
    assert!(parse(&mask.replace("Negative", "Positive")).is_err());
    let profile = header("Profile,NP", "Positive") + "%ADD10C,0*%D10*X0Y0D02*X1000001Y-2000003D01*M02*";
    assert_eq!(
        parse(&profile).unwrap().primitives,
        vec![P::Segment { start: Point::new(Nm(0), Nm(0)), end: Point::new(Nm(1000001), Nm(2000003)), width: Nm(0) }]
    );
    assert!(parse(&profile.replace("Profile,NP", "Copper,L1,Top")).is_err());
    let xnc = "; original asymmetric drill\r\nM48\r\nMETRIC\r\nT01C0.300001000\r\n%\r\nG05\r\nT01\r\nX-1.000001Y2.000003\r\nM30\r\n";
    let drill = view::parse_xnc(xnc, &mut FabricationReadBudget::default()).unwrap();
    assert_eq!(drill.layer, L::Drill);
    assert!(drill.unverified_common_origin);
    assert_eq!(drill.primitives, vec![P::Circle { center: Point::new(Nm(-1000001), Nm(-2000003)), diameter: Nm(300001) }]);
    for replacement in ["INCH", "METRIC\nT01C0.4", "METRIC\nG00", "METRIC\nT02C0.0000001"] {
        assert!(view::parse_xnc(&xnc.replace("METRIC", replacement), &mut FabricationReadBudget::default()).is_err());
    }
    assert!(view::parse_xnc(&xnc.replace("M30", "M30\nT01"), &mut FabricationReadBudget::default()).is_err());
}
#[test]
fn malformed_unsupported_and_ambiguous_graphics_never_return_a_partial_picture() {
    let base = header("Copper,L1,Top", "Positive") + "%ADD10C,0.25*%D10*X100Y200D03*M02*";
    for replacement in ["%AMX*%", "%LPC*%", "G02*", "G36*", "%LS2*%", "%LMX*%", "%TF.Unknown,x*%", "%TO.N,N1*%"] {
        assert!(parse(&base.replace("D10*", &format!("{replacement}D10*"))).is_err(), "{replacement}");
    }
    for changed in [
        base.replace("M02*", ""),
        base.clone() + "G01*",
        base.replace("D10*", "D11*"),
        base.replace("X100Y200D03", "X100D03"),
        base.replace("%MOMM*%", "%MOMM*%%MOMM*%"),
        base.replace("X100Y200D03", "X100000000000Y0D03"),
        base.replace("0.25", "0.0000001"),
    ] {
        assert!(parse(&changed).is_err());
    }
    assert!(parse(&(header("Copper,L1,Top", "Positive") + "%ADD10R,1X2*%D10*X0Y0D02*X100Y200D01*M02*")).is_err());
}
#[test]
fn aggregate_budget_is_shared_across_files_and_exact_boundary_does_not_truncate() {
    let mut budget = FabricationReadBudget::default();
    let pre = header("Copper,L1,Top", "Positive") + "%ADD10C,0.25*%D10*";
    let first = pre.clone() + &"X1Y2D03*".repeat(5000) + "M02*";
    assert_eq!(view::parse_gerber(&first, &mut budget).unwrap().primitives.len(), 5000);
    assert_eq!(view::parse_gerber(&first, &mut budget).unwrap().primitives.len(), 5000);
    assert!(view::parse_gerber(&(pre + "X1Y2D03*M02*"), &mut budget).unwrap_err().contains("10000 primitives"));
    assert!(parse(&format!("G04 {}*", "x".repeat(4097))).is_err());
    assert!(parse(&" ".repeat(view::MAX_FILE_BYTES + 1)).is_err());
    let comments = "G04 x*".repeat(view::MAX_COMMANDS + 1);
    assert!(parse(&comments).unwrap_err().contains("100000 commands"));
    let definitions = (10..267).map(|id| format!("%ADD{id}C,1*%")).collect::<String>();
    assert!(parse(&(header("Copper,L1,Top", "Positive") + &definitions + "M02*")).is_err());
}
