use viamind_doc::{
    CopperLayer,
    pcb_sexpr::{self, Disposition},
};
use viamind_geom::{Nm, Point};
const SOURCE: &str = include_str!("../../../fixtures/import/original-two-layer.kicad_pcb");
#[test]
fn original_asymmetric_exact_nm_geometry_nets_ids_and_complete_fidelity() {
    let parsed = pcb_sexpr::parse(SOURCE).unwrap();
    assert!(parsed.report.can_import(), "{:?}", parsed.report.items.iter().filter(|i| i.disposition == Disposition::Blocked).collect::<Vec<_>>());
    assert!(!parsed.report.full_fidelity);
    assert_eq!(parsed.report.imported.parts, 1);
    assert_eq!(parsed.report.imported.pads, 2);
    assert_eq!(parsed.report.imported.tracks, 2);
    assert_eq!(parsed.report.imported.vias, 1);
    let p = parsed.project.board.parts.values().next().unwrap();
    assert_eq!(p.designator, "U1");
    assert_eq!(p.value, "Original \"quoted\" value");
    assert_eq!(p.id.to_string(), "20000000-0000-4000-8000-000000000001");
    let pad = &p.pads.as_ref().unwrap().0[0];
    assert_eq!(pad.net.as_deref(), Some("VIN"));
    assert_eq!(p.local_to_world_nm_exact(pad.position).unwrap(), Point::new(Nm(3_500_004), Nm(5_250_012)));
    let outline = parsed.project.board.outline.unwrap();
    assert_eq!(outline.cutouts.len(), 1);
    assert!(outline.outer.points.contains(&Point::new(Nm(-1_000_001), Nm(-2_000_003))));
    assert!(parsed.project.board.tracks.values().any(|t| t.layer == CopperLayer::Bottom));
    assert!(parsed.report.items.iter().any(|i| i.token == "layers" && i.disposition == Disposition::Omitted));
    let outline_item = parsed.report.items.iter().find(|i| i.token == "gr_rect").unwrap();
    assert!(outline_item.source_id.is_some());
    assert!(
        parsed
            .report
            .items
            .iter()
            .any(|i| i.path.starts_with(&format!("{}/", outline_item.path)) && i.token == "uuid" && i.disposition == Disposition::Omitted)
    );
    assert!(parsed.report.items.iter().any(|i| i.token == "uuid" && i.disposition == Disposition::Imported));
    let again = pcb_sexpr::parse(SOURCE).unwrap();
    assert_eq!(parsed.report, again.report);
    let bom = pcb_sexpr::parse(&format!("\u{feff}{SOURCE}")).unwrap();
    assert_eq!(bom.report.items[0].byte_offset, 3);
    for (without, with) in parsed.report.items.iter().zip(&bom.report.items) {
        assert_eq!(with.byte_offset, without.byte_offset + 3);
    }
}
#[test]
fn tested_header_and_identity_alias_grammars_are_supported() {
    for version in ["20221018", "20240108", "20241229"] {
        let s = SOURCE.replace("20241229", version).replace("(uuid ", "(tstamp ");
        assert!(pcb_sexpr::parse(&s).unwrap().report.can_import());
    }
    let p = pcb_sexpr::parse(&SOURCE.replace("(net 1)", "(net 0)")).unwrap();
    assert!(p.project.board.tracks.values().any(|t| t.net.is_none()));
}
#[test]
fn unsupported_copper_transforms_semantics_and_outline_never_look_complete() {
    for (from, to) in [
        ("smd rect", "smd roundrect"),
        ("(at 4.000007 5.000011)", "(at 4.000007 5.000011 90)"),
        ("(at -0.500003 0.250001)", "(at -0.500003 0.250001 90)"),
        ("(size 0.6)", "(size 0.6) (remove_unused_layers)"),
        ("(net 1 \"VIN\")", "(net 1 \"WRONG\")"),
        ("(width 0.2)", "(width 0.2) (clearance 0.5)"),
        ("(layer \"B.Cu\")", "(layer \"In1.Cu\")"),
        ("(start -1.000001 -2.000003)", "(start -1.000001 -2.000003) (arc_extra 1)"),
        ("(end 20 15)", "(end -1.000001 15)"),
        ("(layer \"F.SilkS\")", "(layer \"F.Cu\")"),
    ] {
        let p = pcb_sexpr::parse(&SOURCE.replacen(from, to, 1)).unwrap();
        assert!(!p.report.can_import(), "{from} -> {to}");
        assert!(p.report.items.iter().any(|i| i.disposition == Disposition::Blocked));
    }
    let s = SOURCE.replacen("(generator", "(zone (net 1)) (generator", 1);
    assert!(pcb_sexpr::parse(&s).unwrap().report.items.iter().any(|i| i.token == "zone" && i.disposition == Disposition::Blocked));
}
#[test]
fn malformed_identity_net_fields_precision_and_resources_reject() {
    for s in [
        SOURCE.replace("20241229", "20991231"),
        SOURCE.replace("50000000-0000-4000-8000-000000000001", "20000000-0000-4000-8000-000000000001"),
        SOURCE.replace("50000000-0000-4000-8000-000000000001", "00000000-0000-0000-0000-000000000000"),
        SOURCE.replace("(net 2 \"GND\")", "(net 2 \"VIN\")"),
        SOURCE.replace("(net 0 \"\")", "(net 0 \"wrong\")"),
        format!("{}{}", "(".repeat(33), ")".repeat(33)),
        format!("(kicad_pcb (foo \"{}\"))", "x".repeat(4097)),
        "x".repeat(pcb_sexpr::MAX_SOURCE_BYTES + 1),
        format!("{SOURCE} trailing"),
    ] {
        assert!(pcb_sexpr::parse(&s).is_err());
    }
    for s in ["nan", "1e2", "0.0000001", "-10000.000001", "9223372036854775807", "1..0", "."] {
        assert!(pcb_sexpr::decimal_nm(s).is_err(), "{s}");
    }
    assert_eq!(pcb_sexpr::decimal_nm("-1.000001000").unwrap(), Nm(-1_000_001));
    assert_eq!(pcb_sexpr::decimal_nm("0.000001").unwrap(), Nm(1));
    let p = pcb_sexpr::parse(&SOURCE.replace("(width 0.2)", "(width 0.2) (width 0.3)")).unwrap();
    assert!(!p.report.can_import());
}

#[test]
fn canonical_layer_table_rejects_ambiguous_arity_ids_types_and_order() {
    for (from, to) in [
        ("(31 \"B.Cu\" signal)", "(0 \"B.Cu\" signal)"),
        ("(0 \"F.Cu\" signal)", "(32 \"F.Cu\" signal)"),
        ("(0 \"F.Cu\" signal)", "(0 \"F.Cu\" invalid)"),
        ("(0 \"F.Cu\" signal)", "(0 \"F.Cu\" signal \"display\" extra)"),
    ] {
        assert!(pcb_sexpr::parse(&SOURCE.replace(from, to)).is_err(), "{from} -> {to}");
    }
    let p = pcb_sexpr::parse(&SOURCE.replace("(44 \"Edge.Cuts\" user)", "(44 \"Edge.Cuts\" user) (1 \"In1.Cu\" signal)")).unwrap();
    assert!(!p.report.can_import());
    assert!(pcb_sexpr::parse(&SOURCE.replace("(0 \"F.Cu\" signal)", "(0 \"F.Cu\" signal \"front\")")).unwrap().report.can_import());
}

#[test]
fn parser_node_token_aggregate_and_physical_work_limits_are_explicit() {
    let cases = [
        (format!("(kicad_pcb {})", "(x)".repeat(20_000)), "object/report count"),
        (format!("(kicad_pcb (x {}))", "a ".repeat(100_000)), "tokens"),
        (format!("(kicad_pcb (x {}))", format!("\"{}\" ", "x".repeat(4096)).repeat(513)), "aggregate"),
        (SOURCE.replacen("(generator", &format!("{} (generator", "(segment)".repeat(10_001)), 1), "physical object count"),
    ];
    for (source, expected) in cases {
        assert!(pcb_sexpr::parse(&source).unwrap_err().to_string().contains(expected), "{expected}");
    }
}

fn line_board(points: &[(i32, i32)]) -> String {
    let mut source = "(kicad_pcb (version 20241229) (layers (0 F.Cu signal) (31 B.Cu signal) (44 Edge.Cuts user)) (net 0 \"\")".to_string();
    for (index, pair) in points.windows(2).enumerate() {
        source.push_str(&format!(
            "(gr_line (start {} {}) (end {} {}) (layer Edge.Cuts) (uuid 60000000-0000-4000-8000-{:012}))",
            pair[0].0,
            pair[0].1,
            pair[1].0,
            pair[1].1,
            index + 1
        ));
    }
    source.push(')');
    source
}
#[test]
fn exact_line_loops_connect_but_open_duplicate_and_crossed_boundaries_block() {
    let points = [(0, 0), (8, 0), (8, 2), (3, 2), (3, 7), (0, 7), (0, 0)];
    let board = pcb_sexpr::parse(&line_board(&points)).unwrap();
    assert!(board.report.can_import());
    assert_eq!(board.project.board.outline.unwrap().outer.points.len(), 6);
    for points in [vec![(0, 0), (8, 0), (8, 2)], vec![(0, 0), (8, 8), (0, 8), (8, 0), (0, 0)], vec![(0, 0), (8, 0), (0, 0)]] {
        let board = pcb_sexpr::parse(&line_board(&points)).unwrap();
        assert!(!board.report.can_import());
        assert!(board.report.items.iter().any(|item| item.disposition == Disposition::Blocked));
    }
    let outside = SOURCE.replace("(start 10 10)", "(start 30 30)").replace("(end 11 11)", "(end 31 31)");
    assert!(!pcb_sexpr::parse(&outside).unwrap().report.can_import());
}
