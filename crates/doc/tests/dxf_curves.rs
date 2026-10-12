//! Independent analytic oracles and admission boundaries for certified circular DXF conversion.
use viamind_doc::dxf::{DxfUnits, import_outline_dxf};
use viamind_doc::dxf_curves::{self as curves, DxfConversion, SegmentKind, convert_outline_dxf};
use viamind_geom::{Nm, Point};

fn dxf(entities: &str) -> String {
    format!("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n")
}
/// Closed LWPOLYLINE with `(x, y, bulge)` vertices in mm ("" = no bulge group).
fn lw(layer: &str, vertices: &[(&str, &str, &str)]) -> String {
    let mut s = format!("0\nLWPOLYLINE\n8\n{layer}\n90\n{}\n70\n1\n", vertices.len());
    for (x, y, b) in vertices {
        s += &format!("10\n{x}\n20\n{y}\n");
        if !b.is_empty() {
            s += &format!("42\n{b}\n");
        }
    }
    s
}
fn line(a: (&str, &str), b: (&str, &str)) -> String {
    format!("0\nLINE\n8\nEdge\n10\n{}\n20\n{}\n11\n{}\n21\n{}\n", a.0, a.1, b.0, b.1)
}
fn arc(c: (&str, &str), r: &str, start: &str, end: &str) -> String {
    format!("0\nARC\n8\nEdge\n10\n{}\n20\n{}\n40\n{r}\n50\n{start}\n51\n{end}\n", c.0, c.1)
}
fn circle(c: (&str, &str), r: &str) -> String {
    format!("0\nCIRCLE\n8\nEdge\n10\n{}\n20\n{}\n40\n{r}\n", c.0, c.1)
}
fn convert(text: &str, tolerance: u64) -> Result<DxfConversion, viamind_doc::dxf::DxfError> {
    convert_outline_dxf(text, DxfUnits::Header, None, tolerance)
}
/// Native point to DXF-frame f64 nm (Y negated back).
fn dxf_xy(p: Point) -> (f64, f64) {
    (p.x.0 as f64, -(p.y.0 as f64))
}
/// Independent supplementary oracle: two-sided sampled distance between the true circle
/// arc spanned by consecutive emitted vertices and their emitted chord.
fn arc_oracle(points: &[Point], center: (f64, f64), radius: f64) -> f64 {
    let mut worst = 0f64;
    let segment_distance = |q: (f64, f64), a: (f64, f64), b: (f64, f64)| {
        let (vx, vy) = (b.0 - a.0, b.1 - a.1);
        let t = (((q.0 - a.0) * vx + (q.1 - a.1) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
        (q.0 - a.0 - t * vx).hypot(q.1 - a.1 - t * vy)
    };
    for w in points.windows(2) {
        let (a, b) = (dxf_xy(w[0]), dxf_xy(w[1]));
        let ta = (a.1 - center.1).atan2(a.0 - center.0);
        let mut sweep = (b.1 - center.1).atan2(b.0 - center.0) - ta;
        if sweep > std::f64::consts::PI {
            sweep -= std::f64::consts::TAU;
        } else if sweep < -std::f64::consts::PI {
            sweep += std::f64::consts::TAU;
        }
        for i in 0..=64 {
            let t = ta + sweep * f64::from(i) / 64.0;
            let q = (center.0 + radius * t.cos(), center.1 + radius * t.sin());
            worst = worst.max(segment_distance(q, a, b));
            let c = (a.0 + (b.0 - a.0) * f64::from(i) / 64.0, a.1 + (b.1 - a.1) * f64::from(i) / 64.0);
            worst = worst.max(((c.0 - center.0).hypot(c.1 - center.1) - radius).abs());
        }
    }
    worst
}
fn ring_points_between(points: &[Point], from: Point, to: Point) -> Vec<Point> {
    let n = points.len();
    let start = points.iter().position(|p| *p == from).unwrap();
    let mut out = vec![from];
    let mut i = start;
    loop {
        i = (i + 1) % n;
        out.push(points[i]);
        if points[i] == to {
            return out;
        }
        assert_ne!(i, start, "endpoint {to:?} not found after {from:?}");
    }
}
fn p(x_mm: f64, y_mm: f64) -> Point {
    Point { x: Nm::from_mm(x_mm), y: Nm::from_mm(-y_mm) }
}

#[test]
fn straight_only_sources_keep_predecessor_polygon_bytes_and_report_only_rounding() {
    for (name, text) in [
        ("mm", include_str!("fixtures/outline-mm.dxf")),
        ("lines", include_str!("fixtures/outline-lines.dxf")),
        ("polyline", include_str!("fixtures/outline-polyline.dxf")),
        ("inch", include_str!("fixtures/outline-inch.dxf")),
    ] {
        let legacy = import_outline_dxf(text, DxfUnits::Header, None).unwrap();
        let converted = convert(text, curves::DEFAULT_TOLERANCE_NM).unwrap();
        assert_eq!(serde_json::to_vec(&converted.outline).unwrap(), serde_json::to_vec(&legacy).unwrap(), "{name}");
        assert!(converted.report.straight_only && converted.report.curved_segments == 0, "{name}");
        assert!(converted.report.rows.iter().all(|r| r.sagitta_nm == 0 && r.chords == 1 && r.kind == SegmentKind::Line));
        assert!(converted.report.notices.iter().any(|n| n.contains("Straight-only")));
    }
    // Sub-Nm source rounding is disclosed as a nonzero quantization bound, never zero.
    let mm = include_str!("fixtures/outline-mm.dxf");
    let odd = mm.replace("10\n40\n", "10\n40.0000006\n");
    let converted = convert(&odd, 10).unwrap();
    assert_eq!(
        serde_json::to_vec(&converted.outline).unwrap(),
        serde_json::to_vec(&import_outline_dxf(&odd, DxfUnits::Header, None).unwrap()).unwrap()
    );
    assert!(converted.report.rows.iter().any(|r| r.quantization_nm == 1));
    assert_eq!(converted.report.achieved_bound_nm, 1);
    let exact = convert(mm, 10).unwrap();
    assert_eq!(exact.report.achieved_bound_nm, 0);
    // Same layer-selection convention and override reporting as the straight importer.
    let mixed = mm.replace("0\nENDSEC\n0\nEOF", "0\nTEXT\n8\nNotes\n1\nhello\n0\nENDSEC\n0\nEOF");
    assert!(convert(&mixed, 10).unwrap_err().to_string().contains("unsupported selected entity TEXT"));
    let filtered = convert_outline_dxf(&mixed, DxfUnits::Mm, Some("Edge"), 10).unwrap();
    assert_eq!(filtered.outline, import_outline_dxf(&mixed, DxfUnits::Header, Some("Edge")).unwrap());
    assert_eq!((filtered.report.ignored_entities, filtered.report.ignored_by_type["TEXT"]), (1, 1));
    assert!(filtered.report.unit_override && filtered.report.header_insunits == Some(4));
    // Determinism of the complete report for identical bytes/options.
    assert_eq!(convert_outline_dxf(&mixed, DxfUnits::Mm, Some("Edge"), 10).unwrap(), filtered);
}

#[test]
fn semicircle_bulges_form_an_exact_signed_circle_within_the_certified_bound() {
    let tolerance = 10_000;
    for sign in ["1", "-1"] {
        let text = dxf(&lw("Edge", &[("0", "0", sign), ("20", "0", sign)]));
        let c = convert(&text, tolerance).unwrap();
        let ring = &c.outline.outer.points;
        // Analytic depth: R(1-cos(pi/64)) = 12045 nm > tol, R(1-cos(pi/128)) = 3012 nm <= tol.
        assert_eq!(ring.len(), 128, "{sign}");
        assert_eq!(c.report.rows.len(), 2);
        assert!(c.report.rows.iter().all(|r| r.chords == 64 && r.kind == SegmentKind::Bulge));
        assert_eq!((ring[0], ring[64]), (p(0.0, 0.0), p(20.0, 0.0)));
        // Positive bulge runs CCW in DXF from (0,0) to (20,0), i.e. through DXF y<0 (native y>0).
        let first_half_native_y = ring[32].y.0;
        assert_eq!(first_half_native_y.signum(), if sign == "1" { 1 } else { -1 }, "{sign}");
        assert_eq!(ring[32], Point { x: Nm(10_000_000), y: Nm(if sign == "1" { 10_000_000 } else { -10_000_000 }) });
        let actual = arc_oracle(&[ring.as_slice(), &ring[..1]].concat(), (10e6, 0.0), 10e6);
        assert!(actual <= c.report.achieved_bound_nm as f64 + 1e-6, "oracle {actual} must not exceed certificate");
        assert!(c.report.achieved_bound_nm <= tolerance);
        assert!(c.report.rows.iter().all(|r| r.sagitta_nm >= 2_900 && r.sagitta_nm <= 3_100), "{:?}", c.report.rows);
    }
}

#[test]
fn rational_bulges_both_directions_match_center_radius_and_sagitta_oracles() {
    // b = 1/2 on a 30 mm chord: r = 18.75 mm, sagitta 7.5 mm, center (15, +-11.25).
    for (bulge, center_y, bulging_y) in [("0.5", 11.25, -7.5), ("-0.5", -11.25, 7.5)] {
        let text = dxf(&lw("Edge", &[("0", "0", bulge), ("30", "0", ""), ("30", "20", ""), ("0", "20", "")]));
        let c = convert(&text, 1_000).unwrap();
        let ring = &c.outline.outer.points;
        let arc = ring_points_between(ring, p(0.0, 0.0), p(30.0, 0.0));
        let actual = arc_oracle(&arc, (15e6, center_y * 1e6), 18.75e6);
        assert!(actual <= c.report.rows[0].bound_nm as f64 + 1e-6 && c.report.rows[0].bound_nm <= 1_000);
        let extreme = arc
            .iter()
            .map(|q| dxf_xy(*q).1)
            .fold(if bulging_y < 0.0 { f64::MAX } else { f64::MIN }, |m, y| if bulging_y < 0.0 { m.min(y) } else { m.max(y) });
        assert_eq!(extreme, bulging_y * 1e6, "the exact arc midpoint is emitted at the first bisection");
        assert_eq!(c.report.rows[1..].iter().map(|r| r.kind).collect::<Vec<_>>(), [SegmentKind::Line; 3]);
    }
}

#[test]
fn quarter_arc_line_loops_wrap_angles_and_keep_counterclockwise_dxf_direction() {
    let rounded = |a0: &str, a1: &str| {
        dxf(&[
            line(("0", "0"), ("30", "0")),
            line(("30", "0"), ("30", "10")),
            arc(("20", "10"), "10", a0, a1),
            line(("20", "20"), ("0", "20")),
            line(("0", "20"), ("0", "0")),
        ]
        .concat())
    };
    let base = convert(&rounded("0", "90"), 500).unwrap();
    let corner = ring_points_between(&base.outline.outer.points, p(30.0, 10.0), p(20.0, 20.0));
    assert!(corner.len() > 2);
    assert!(corner.iter().all(|q| {
        let (x, y) = dxf_xy(*q);
        x >= 20e6 && y >= 10e6
    }));
    assert!(arc_oracle(&corner, (20e6, 10e6), 10e6) <= base.report.achieved_bound_nm as f64 + 1e-6);
    for (a0, a1) in [("360", "90"), ("-360", "90"), ("0.0", "-270")] {
        assert_eq!(convert(&rounded(a0, a1), 500).unwrap().outline, base.outline);
    }
    // Wrapping across 0 degrees: 270..0, -90..0, -90..360 and 270..360 are one quarter.
    let wrap = |a0: &str, a1: &str| dxf(&[line(("0", "0"), ("10", "-10")), arc(("10", "0"), "10", a0, a1), line(("20", "0"), ("0", "0"))].concat());
    let reference = convert(&wrap("270", "0"), 200).unwrap();
    for (a0, a1) in [("-90", "0"), ("-90", "360"), ("270", "360"), ("-90.000", "0e0")] {
        assert_eq!(convert(&wrap(a0, a1), 200).unwrap().outline, reference.outline, "{a0}..{a1}");
    }
    for (a0, a1) in [("0", "0"), ("0", "360"), ("90", "-270"), ("0", "360.0000001"), ("0", "NaN")] {
        assert!(convert(&wrap(a0, a1), 200).is_err(), "{a0}..{a1}");
    }
}

#[test]
fn asymmetric_odd_nm_circle_cutout_keeps_y_orientation_and_exact_quadrant_points() {
    let text =
        dxf(&[lw("Edge", &[("0", "0", ""), ("40", "0", ""), ("40", "30", ""), ("0", "30", "")]), circle(("10.000001", "20.000003"), "5.000007")]
            .concat());
    let c = convert(&text, 1_000).unwrap();
    assert_eq!(c.outline.cutouts.len(), 1);
    let hole = &c.outline.cutouts[0].points;
    // CIRCLE starts at +X and proceeds CCW in DXF; Y is negated once.
    assert_eq!(hole[0], Point { x: Nm(15_000_008), y: Nm(-20_000_003) });
    let quarter = hole.iter().position(|q| *q == Point { x: Nm(10_000_001), y: Nm(-25_000_010) }).unwrap();
    assert_eq!(hole.len() % 4, 0);
    assert_eq!(quarter, hole.len() / 4);
    assert!(hole.contains(&Point { x: Nm(4_999_994), y: Nm(-20_000_003) }) && hole.contains(&Point { x: Nm(10_000_001), y: Nm(-14_999_996) }));
    let closed = [hole.as_slice(), &hole[..1]].concat();
    let actual = arc_oracle(&closed, (10_000_001.0, 20_000_003.0), 5_000_007.0);
    assert!(actual <= c.report.achieved_bound_nm as f64 + 1e-6 && c.report.achieved_bound_nm <= 1_000);
    assert_eq!(c.report.rows[4].kind, SegmentKind::Circle);
    assert_eq!(c.report.bounds_nm.min_y, -30_000_000);
}

#[test]
fn rounded_slot_ring_area_and_reversed_or_reordered_sources_preserve_classification() {
    let slot =
        [line(("0", "0"), ("20", "0")), arc(("20", "5"), "5", "270", "90"), line(("20", "10"), ("0", "10")), arc(("0", "5"), "5", "90", "270")];
    let outer = lw("Edge", &[("-10", "-10", ""), ("30", "-10", ""), ("30", "20", ""), ("-10", "20", "")]);
    let c = convert(&dxf(&[outer.clone(), slot.concat()].concat()), 2_000).unwrap();
    assert_eq!(c.outline.cutouts.len(), 1);
    let hole = &c.outline.cutouts[0];
    let exact_area = 20e6 * 10e6 + std::f64::consts::PI * 25e12;
    let perimeter = 40e6 + std::f64::consts::TAU * 5e6;
    assert!((hole.area() - exact_area).abs() <= perimeter * c.report.achieved_bound_nm as f64);
    assert!(c.report.rows.iter().filter(|r| r.kind == SegmentKind::Arc).count() == 2);
    // Source reorder: same rounded vertex set and classification.
    let reordered = convert(&dxf(&[slot[3].clone(), slot[1].clone(), outer.clone(), slot[2].clone(), slot[0].clone()].concat()), 2_000).unwrap();
    let set = |o: &viamind_doc::BoardOutline| {
        let mut v: Vec<_> = o.cutouts[0].points.iter().map(|q| (q.x.0, q.y.0)).collect();
        v.sort_unstable();
        v
    };
    assert_eq!(set(&reordered.outline), set(&c.outline));
    assert_eq!(reordered.outline.outer, c.outline.outer);
    // Reversed polyline winding with negated bulges emits the identical vertex set.
    let forward = lw("Edge", &[("0", "0", "0.25"), ("10", "0", ""), ("10", "5", "-0.5"), ("0", "5", "")]);
    let reverse = lw("Edge", &[("0", "5", "0.5"), ("10", "5", ""), ("10", "0", "-0.25"), ("0", "0", "")]);
    let (f, r) = (convert(&dxf(&forward), 100).unwrap(), convert(&dxf(&reverse), 100).unwrap());
    let sorted = |o: &viamind_doc::BoardOutline| {
        let mut v: Vec<_> = o.outer.points.iter().map(|q| (q.x.0, q.y.0)).collect();
        v.sort_unstable();
        v
    };
    assert_eq!(sorted(&f.outline), sorted(&r.outline));
    assert_eq!(f.outline.outer.signed_area_x2(), -r.outline.outer.signed_area_x2());
}

#[test]
fn every_source_budget_admits_its_boundary_and_refuses_max_plus_one() {
    let square = lw("Edge", &[("0", "0", ""), ("40", "0", ""), ("40", "30", ""), ("0", "30", "")]);
    let base = dxf(&square);
    // Tolerance.
    for (t, ok) in [(9, false), (10, true), (1_000_000, true), (1_000_001, false)] {
        assert_eq!(convert(&base, t).is_ok(), ok, "{t}");
    }
    // Pairs including comments: the square uses `pairs` pairs; pad with comments to the limit.
    let pairs = base.lines().count() / 2;
    let pad = |extra: usize| base.replacen("0\nSECTION", &format!("{}0\nSECTION", "999\nc\n".repeat(extra)), 1);
    assert!(convert(&pad(curves::MAX_PAIRS - pairs), 10).is_ok());
    assert!(convert(&pad(curves::MAX_PAIRS - pairs + 1), 10).unwrap_err().to_string().contains("131072"));
    // Line length.
    let long = |n: usize| base.replacen("0\nSECTION", &format!("999\n{}\n0\nSECTION", "x".repeat(n)), 1);
    assert!(convert(&long(4096), 10).is_ok());
    assert!(convert(&long(4097), 10).unwrap_err().to_string().contains("4096"));
    // Numeric token, significant digits and exponent.
    let token = |v: &str| dxf(&lw("Edge", &[("0", "0", ""), (v, "0", ""), ("40", "30", ""), ("0", "30", "")]));
    assert!(convert(&token(&format!("40.{}", "0".repeat(61))), 10).is_ok()); // 64 bytes
    assert!(convert(&token(&format!("40.{}", "0".repeat(62))), 10).is_err());
    assert!(convert(&token(&format!("4.{}1", "0".repeat(30))), 10).is_ok()); // 32 significant digits
    assert!(convert(&token(&format!("4.{}1", "0".repeat(31))), 10).is_err()); // 33 significant digits
    assert!(convert(&token("0.0000000000000000000000000000004e32"), 10).is_ok());
    for bad in ["4e33", "40e-33", "NaN", "inf", "0x28", "4.0.0", "1e400"] {
        assert!(convert(&token(bad), 10).is_err(), "{bad}");
    }
    // Layer names.
    let named = |n: usize| dxf(&lw(&"L".repeat(n), &[("0", "0", ""), ("40", "0", ""), ("40", "30", ""), ("0", "30", "")]));
    assert!(convert(&named(512), 10).is_ok());
    assert!(convert(&named(513), 10).unwrap_err().to_string().contains("512"));
    assert!(convert_outline_dxf(&base, DxfUnits::Header, Some(&"L".repeat(513)), 10).is_err());
    // Entity records, including unselected ones.
    let records = |n: usize| dxf(&format!("{square}{}", "0\nTEXT\n8\nNotes\n".repeat(n - 1)));
    assert!(convert_outline_dxf(&records(curves::MAX_ENTITY_RECORDS), DxfUnits::Header, Some("Edge"), 10).is_ok());
    assert!(
        convert_outline_dxf(&records(curves::MAX_ENTITY_RECORDS + 1), DxfUnits::Header, Some("Edge"), 10).unwrap_err().to_string().contains("16384")
    );
    // Segments and vertices: a 2048-gon is admitted, 2049 refused before validation.
    let polygon = |n: usize| {
        let pts: Vec<(String, String)> = (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                (format!("{:.6}", 1000.0 * a.cos()), format!("{:.6}", 1000.0 * a.sin()))
            })
            .collect();
        let refs: Vec<(&str, &str, &str)> = pts.iter().map(|(x, y)| (x.as_str(), y.as_str(), "")).collect();
        dxf(&lw("Edge", &refs))
    };
    assert_eq!(convert(&polygon(2048), 10).unwrap().report.vertices, 2048);
    assert!(convert(&polygon(2049), 10).unwrap_err().to_string().contains("2048"));
    // Rings: 1 outer + 64 cutouts admitted, 65 cutouts refused.
    let holes = |n: usize| {
        let mut s = lw("Edge", &[("0", "0", ""), ("1000", "0", ""), ("1000", "1000", ""), ("0", "1000", "")]);
        for i in 0..n {
            s += &circle((&format!("{}", 50 + (i % 9) * 100), &format!("{}", 50 + (i / 9) * 100)), "10");
        }
        dxf(&s)
    };
    assert_eq!(convert(&holes(64), 1_000_000).unwrap().report.cutouts, 64);
    assert!(convert(&holes(65), 1_000_000).unwrap_err().to_string().contains("65"));
    // Source bytes.
    assert!(convert(&" ".repeat(curves::MAX_SOURCE_BYTES + 1), 10).unwrap_err().to_string().contains("8 MiB"));
    // Bounded diagnostics never exceed 512 bytes even for oversized untrusted names.
    let error = convert(&dxf(&format!("0\n{}\n8\nEdge\n", "X".repeat(4000))), 10).unwrap_err().to_string();
    assert!(error.len() <= curves::MAX_DIAGNOSTIC_BYTES, "{}", error.len());
}

#[test]
fn unsupported_ill_conditioned_and_out_of_domain_geometry_refuses_without_partial_output() {
    let rect = |extra: &str| dxf(&[lw("Edge", &[("0", "0", ""), ("40", "0", ""), ("40", "30", ""), ("0", "30", "")]), extra.to_owned()].concat());
    let cases: Vec<(String, &str)> = vec![
        (dxf(&lw("Edge", &[("0", "0", "1.0000001"), ("20", "0", "1")])), "split"),
        (dxf(&lw("Edge", &[("0", "0", "1e-20"), ("20", "0", ""), ("20", "20", "")])), "ill-conditioned"),
        (dxf(&lw("Edge", &[("0", "0", "0.5"), ("20", "0", "0.5"), ("0", "0", "0.5")])), "repeats the first"),
        (dxf(&lw("Edge", &[("0", "0", ""), ("0.0000001", "0", ""), ("20", "20", "")])), "coincide"),
        (dxf(&lw("Edge", &[("0", "0", "0.5"), ("20", "0", "")]).replace("42\n0.5\n", "42\n0.5\n42\n0.5\n")), "duplicate bulge"),
        (rect(&circle(("38", "15"), "5")), "cutouts"),
        (rect(&[circle(("10", "10"), "5"), circle(("18", "10"), "5")].concat()), "cutouts"),
        (rect(&circle(("10", "10"), "-5")), "positive"),
        (rect(&circle(("10", "10"), "0")), "positive"),
        (rect(&circle(("10", "10"), "5").replace("40\n5\n", "40\n5\n40\n5\n")), "duplicate"),
        (dxf(&circle(("9999", "0"), "2")), "domain"),
        (dxf(&circle(("0", "0"), "5000")), "2048"),
        (rect(&circle(("10", "10"), "5").replace("40\n5\n", "40\n5\n230\n-1\n")), "extrusion"),
        (rect(&circle(("10", "10"), "5").replace("40\n5\n", "40\n5\n39\n1\n")), "thickness"),
        (dxf(&lw("Edge", &[("0", "0", ""), ("40", "0", ""), ("40", "30", "")]).replace("70\n1\n", "70\n1\n38\n1\n")), "elevation"),
        (dxf(&lw("Edge", &[("0", "0", ""), ("40", "0", ""), ("40", "30", "")]).replace("70\n1\n", "70\n1\n43\n0.5\n")), "width"),
        (dxf(&lw("Edge", &[("0", "0", ""), ("40", "0", ""), ("40", "30", "")]).replace("70\n1\n", "70\n0\n")), "closed"),
        (rect("0\nELLIPSE\n8\nEdge\n"), "unsupported selected entity ELLIPSE"),
        (rect("0\nSPLINE\n8\nEdge\n"), "unsupported selected entity SPLINE"),
        (rect("0\nINSERT\n8\nEdge\n"), "unsupported selected entity INSERT"),
        (dxf(&[line(("0", "0"), ("20", "0")), arc(("20", "5"), "5", "270", "90"), line(("20", "10"), ("0", "10"))].concat()), "exactly two"),
        (
            dxf(&[line(("0", "0"), ("20", "0")), arc(("20", "5"), "5", "270", "89"), line(("20", "10"), ("0", "10")), line(("0", "10"), ("0", "0"))]
                .concat()),
            "exactly two",
        ),
        (
            dxf(&[line(("0", "0"), ("10", "0")), line(("10", "0"), ("0", "10")), line(("0", "10"), ("12", "12")), line(("12", "12"), ("0", "0"))]
                .concat()),
            "cross",
        ),
    ];
    for (text, needle) in cases {
        let error = convert(&text, 1_000).expect_err(needle).to_string();
        assert!(error.starts_with("DXF outline:") && error.contains(needle), "{needle}: {error}");
        assert!(error.len() <= curves::MAX_DIAGNOSTIC_BYTES);
    }
}
