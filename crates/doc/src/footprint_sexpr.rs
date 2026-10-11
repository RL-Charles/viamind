//! Independent bounded detached footprint conversion; no project identities or nets.
use crate::{
    footprint::{Footprint, FootprintPad, LocalPosition},
    pcb_sexpr::{
        Disposition, FidelityItem, FidelityReport, ImportError, ImportedCounts, Node, allowed, at_zero, atoms, canonical_layer,
        cardinal_pad_geometry, child, mark, name, omit_scalars, parse_tree, subtree, validate_ids,
    },
};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("Footprint conversion: {0}")]
pub struct FootprintConversionError(pub String);
impl From<ImportError> for FootprintConversionError {
    fn from(error: ImportError) -> Self {
        Self(error.0)
    }
}
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
#[derive(Clone, Debug)]
pub struct ParsedFootprint {
    /// Present only when every electrical field can be represented exactly.
    pub footprint: Option<Footprint>,
    pub report: FidelityReport,
}
type Result<T> = std::result::Result<T, FootprintConversionError>;
fn bad(message: impl Into<String>) -> FootprintConversionError {
    FootprintConversionError(message.into())
}
fn detached_ids(n: &Node, items: &mut [FidelityItem]) {
    if matches!(n.head.as_str(), "uuid" | "tstamp") {
        mark(items, n, Disposition::Omitted, "Source UUID is report provenance only; detached native definitions carry no object identities.");
    }
    for c in &n.children {
        detached_ids(c, items);
    }
}
fn pad(n: &Node, items: &mut [FidelityItem]) -> Result<FootprintPad> {
    if n.args.len() != 3 || n.args[1] != "smd" || n.args[2] != "rect" {
        return Err(bad("Only detached rectangular smd pads are supported; drilled/other copper shapes block conversion."));
    }
    allowed(n, &["at", "size", "layers", "net", "uuid", "tstamp", "pinfunction", "pintype"], &[], items)?;
    omit_scalars(n, &["pinfunction", "pintype"], items)?;
    if let Some(net) = child(n, "net")? {
        if atoms(net, 2)? != ["0", ""] {
            return Err(bad("Meaningful net assignment cannot be discarded into a detached library; use an unassigned footprint source."));
        }
        mark(items, net, Disposition::Omitted, "Explicit unassigned net marker omitted; native library has no net assignments.");
    }
    let layers = child(n, "layers")?.ok_or_else(|| bad("Missing explicit front pad layers."))?;
    if !layers.children.is_empty()
        || !layers.args.iter().any(|s| s == "F.Cu")
        || layers.args.iter().any(|s| !matches!(s.as_str(), "F.Cu" | "F.Mask" | "F.Paste"))
        || layers.args.iter().collect::<BTreeSet<_>>().len() != layers.args.len()
    {
        return Err(bad("Front pads require F.Cu with optional unique F.Mask/F.Paste memberships only."));
    }
    mark(
        items,
        layers,
        if layers.args.len() == 1 { Disposition::Imported } else { Disposition::Omitted },
        "Front rectangular copper retained; source mask/paste memberships are omitted and native output defaults may differ.",
    );
    let (position, width, height) = cardinal_pad_geometry(n, items)?;
    let pad = FootprintPad { number: name(&n.args[0])?, position: LocalPosition { x: position.x, y: position.y }, width, height, apertures: None };
    if pad.width.0 <= 0 || pad.height.0 <= 0 {
        return Err(bad("Rectangular pad dimensions must be greater than zero."));
    }
    detached_ids(n, items);
    Ok(pad)
}
fn known_descendants(n: &Node, fields: &[&str]) -> Result<()> {
    for c in &n.children {
        if !fields.contains(&c.head.as_str()) {
            return Err(bad("Unknown semantic child inside recognized omitted metadata/art blocks conversion."));
        }
        known_descendants(c, fields)?;
    }
    Ok(())
}
fn art(n: &Node, items: &mut [FidelityItem]) -> Result<()> {
    known_descendants(
        n,
        &[
            "at",
            "start",
            "end",
            "center",
            "mid",
            "layer",
            "effects",
            "font",
            "face",
            "size",
            "thickness",
            "justify",
            "hide",
            "bold",
            "italic",
            "line_spacing",
            "stroke",
            "width",
            "type",
            "color",
            "pts",
            "xy",
            "fill",
            "uuid",
            "tstamp",
            "render_cache",
            "polygon",
            "glyph",
        ],
    )?;
    let layer = child(n, "layer")?.ok_or_else(|| bad("Graphic/text omission needs an explicit canonical noncopper layer."))?;
    let layer = &atoms(layer, 1)?[0];
    if !canonical_layer(layer) || layer.ends_with(".Cu") || layer == "Edge.Cuts" {
        return Err(bad("Unsupported copper/outline or unknown graphic layer blocks conversion."));
    }
    subtree(items, n, Disposition::Omitted, "Noncopper text/graphic art and its attributes are not represented by the detached native pad library.");
    Ok(())
}
/// Parse one standalone footprint without external files, inferred identities or model changes.
pub fn parse(source: &str) -> Result<ParsedFootprint> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(bad("Detached footprint source exceeds 1 MiB."));
    }
    let (root, mut items) = parse_tree(source)?;
    if root.head != "footprint" || root.args.len() != 1 {
        return Err(bad("Expected one standalone footprint with exactly one name and no placement flags."));
    }
    let footprint_name = name(&root.args[0])?;
    let version_node = child(&root, "version")?.ok_or_else(|| bad("Missing footprint format date."))?;
    let version = atoms(version_node, 1)?[0].clone();
    if !matches!(version.as_str(), "20221018" | "20240108" | "20241229") {
        return Err(bad("Unsupported tested format date; use 20221018,20240108 or20241229."));
    }
    let mut identities = BTreeSet::new();
    validate_ids(&root, &mut identities, &mut items)?;
    // Check all electrical pads before collecting native rectangles, including nested invalid ones.
    fn count_pads(n: &Node) -> usize {
        usize::from(n.head == "pad") + n.children.iter().map(count_pads).sum::<usize>()
    }
    if count_pads(&root) > 1024 {
        return Err(bad("Footprint source exceeds 1024 pads."));
    }
    let mut footprint = Footprint { artwork: None, name: footprint_name, pads: Vec::new() };
    mark(
        &mut items,
        &root,
        Disposition::Imported,
        "Exact supported detached local pad geometry; no project, connectivity or manufacturing fidelity claim.",
    );
    let mut numbers = BTreeSet::new();
    let mut properties = BTreeSet::new();
    for n in &root.children {
        let result: Result<()> = (|| match n.head.as_str() {
            "version" => {
                mark(&mut items, n, Disposition::Omitted, "Source format date retained in report; native library uses its own envelope version.");
                Ok(())
            }
            "layer" => {
                if atoms(n, 1)? != ["F.Cu"] {
                    Err(bad("Only explicit front F.Cu detached geometry is supported."))
                } else {
                    mark(
                        &mut items,
                        n,
                        Disposition::Omitted,
                        "Source front template layer is not stored; native application retains the target component side.",
                    );
                    Ok(())
                }
            }
            "at" => {
                let position = at_zero(&root, &mut items)?;
                if position.x.0 != 0 || position.y.0 != 0 {
                    Err(bad("Nonzero board placement cannot become a detached footprint; export local-origin geometry."))
                } else {
                    mark(&mut items, n, Disposition::Omitted, "Explicit zero/origin placement omitted; detached library has no board placement.");
                    Ok(())
                }
            }
            "uuid" | "tstamp" => {
                detached_ids(n, &mut items);
                Ok(())
            }
            "pad" => pad(n, &mut items).and_then(|p| {
                if !numbers.insert(p.number.clone()) {
                    return Err(bad("Repeated pad numbers cannot be merged into native library geometry."));
                }
                footprint.pads.push(p);
                mark(
                    &mut items,
                    n,
                    Disposition::Imported,
                    "Exact detached numbered rectangular SMD pad; native identity/net assigned only during later explicit application.",
                );
                Ok(())
            }),
            "attr" => {
                if atoms(n, 1)? != ["smd"] {
                    Err(bad("Only simple smd footprint attributes are supported; other population/electrical flags block conversion."))
                } else {
                    mark(
                        &mut items,
                        n,
                        Disposition::Omitted,
                        "Source SMD classification implied by supported native rectangles; other attributes are not inferred.",
                    );
                    Ok(())
                }
            }
            "property" => {
                if n.args.len() != 2 {
                    Err(bad("Property needs exactly key/value; unknown scalar property semantics block conversion."))
                } else if !properties.insert(n.args[0].clone()) {
                    Err(bad("Repeated source property keys are ambiguous metadata."))
                } else if n.children.is_empty() {
                    subtree(
                        &mut items,
                        n,
                        Disposition::Omitted,
                        "Source property metadata is not retained or interpreted as native identity, connectivity or sourcing.",
                    );
                    Ok(())
                } else {
                    art(n, &mut items)
                }
            }
            "fp_text" | "fp_text_box" | "fp_line" | "fp_rect" | "fp_circle" | "fp_arc" | "fp_poly" | "fp_curve" => art(n, &mut items),
            "model" => {
                if n.args.len() != 1 {
                    return Err(bad("Model metadata needs exactly one source filename."));
                }
                known_descendants(n, &["at", "offset", "scale", "rotate", "xyz"])?;
                subtree(&mut items, n, Disposition::Omitted, "Model metadata omitted; external file is never read or resolved.");
                Ok(())
            }
            "generator" | "generator_version" | "tedit" | "descr" | "tags" => {
                atoms(n, 1)?;
                subtree(
                    &mut items,
                    n,
                    Disposition::Omitted,
                    "Recognized generator/edit/search/model metadata is not retained; no external model/library reads occur.",
                );
                Ok(())
            }
            _ => Err(bad("Unsupported footprint object/attribute; inspect its exact token and source path.")),
        })();
        if let Err(error) = result {
            subtree(&mut items, n, Disposition::Blocked, &error.0);
        }
    }
    if child(&root, "layer")?.is_none() {
        mark(&mut items, &root, Disposition::Blocked, "Explicit F.Cu template layer is required.");
    }
    if footprint.pads.is_empty() {
        mark(&mut items, &root, Disposition::Blocked, "At least one supported electrical pad is required.");
    }
    if let Err(error) = footprint.validate() {
        mark(&mut items, &root, Disposition::Blocked, &error);
    }
    let omitted_lists = items.iter().filter(|i| i.disposition == Disposition::Omitted).count();
    let blocked_lists = items.iter().filter(|i| i.disposition == Disposition::Blocked).count();
    let report = FidelityReport {
        format_version: version,
        imported: ImportedCounts { pads: footprint.pads.len(), ..Default::default() },
        omitted_lists,
        blocked_lists,
        full_fidelity: false,
        items,
    };
    Ok(ParsedFootprint { footprint: if report.can_import() { Some(footprint) } else { None }, report })
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = include_str!("../../../fixtures/import/original-rect-smd.kicad_mod");
    #[test]
    fn exact_original_detached_geometry_and_full_source_report_are_deterministic() {
        let parsed = parse(FIXTURE).unwrap();
        let footprint = parsed.footprint.unwrap();
        assert_eq!(footprint.name, "OriginalRect2");
        assert_eq!(footprint.pads.len(), 2);
        assert_eq!(footprint.pads[0].position.x.0, -500003);
        assert_eq!(footprint.pads[0].position.y.0, 250001);
        assert_eq!(footprint.pads[1].position.x.0, 750007);
        assert_eq!(footprint.pads[1].position.y.0, -125003);
        assert_eq!(footprint.pads[1].width.0, 1000001);
        assert_eq!(footprint.pads[1].height.0, 600003);
        let bytes = crate::footprint::to_vec(&footprint).unwrap();
        assert_eq!(crate::footprint::from_slice(&bytes).unwrap(), footprint);
        let output = String::from_utf8(bytes).unwrap();
        assert!(!output.contains("uuid") && !output.contains("net") && !output.contains("layer"));
        assert_eq!(parsed.report, parse(FIXTURE).unwrap().report);
        assert_eq!(parsed.report.imported.pads, 2);
        assert_eq!(parsed.report.imported.parts, 0);
        assert!(!parsed.report.full_fidelity && parsed.report.can_import());
        assert!(parsed.report.items.iter().any(|i| i.source_id.is_some()));
        assert!(parsed.report.items.iter().filter(|i| i.token == "uuid").all(|i| i.disposition == Disposition::Omitted));
        assert!(parsed.report.items.iter().all(|i| !i.reason.is_empty()));
        let bom = parse(&format!("\u{feff}{FIXTURE}")).unwrap();
        for (a, b) in parsed.report.items.iter().zip(&bom.report.items) {
            assert_eq!(a.path, b.path);
            assert_eq!(a.byte_offset + 3, b.byte_offset);
        }
    }
    #[test]
    fn tested_headers_optional_source_ids_unassigned_markers_and_zero_origin() {
        for version in ["20221018", "20240108", "20241229"] {
            let source = FIXTURE
                .replace("20241229", version)
                .replace("(uuid \"70000000-0000-4000-8000-000000000001\")", "")
                .replace("(net 0 \"\")", "")
                .replace("(layer \"F.Cu\")", "(layer \"F.Cu\") (at 0 -0.000000 0)");
            assert!(parse(&source).unwrap().report.can_import());
        }
    }
    fn blocked(source: &str) {
        let result = parse(source).unwrap();
        assert!(result.footprint.is_none());
        assert!(result.report.blocked_lists > 0 && !result.report.full_fidelity);
        assert!(result.report.items.iter().any(|i| i.disposition == Disposition::Blocked && !i.reason.is_empty()));
    }
    #[test]
    fn unsupported_geometry_nets_placement_and_unknown_semantics_never_export_partial_library() {
        for (from, to) in [
            ("smd rect", "smd roundrect"),
            ("smd rect", "thru_hole rect"),
            ("(at -0.500003 0.250001)", "(at -0.500003 0.250001 45)"),
            ("(size 0.8 1.2)", "(size 0.8 1.2) (drill 0.4)"),
            ("(size 0.8 1.2)", "(size 0.8 1.2) (clearance 0.3)"),
            ("(layer \"F.Cu\")", "(layer \"B.Cu\")"),
            ("(net 0 \"\")", "(net 1 \"VIN\")"),
            ("(layer \"F.Cu\")", "(layer \"F.Cu\") (at 1 0)"),
            ("(layer \"F.Cu\")", "(layer \"F.Cu\") (at 0 0 90)"),
            ("(layer \"F.CrtYd\")", "(layer \"F.Cu\")"),
            ("(layer \"F.CrtYd\")", "(layer \"Edge.Cuts\")"),
            ("(layer \"F.CrtYd\")", "(layer \"Unknown\")"),
            ("(pad \"2\"", "(pad \"1\""),
            ("(layers \"F.Cu\" \"F.Mask\" \"F.Paste\")", "(layers \"*.Cu\" \"*.Mask\")"),
            ("(layers \"F.Cu\" \"F.Mask\" \"F.Paste\")", "(layers \"F.Cu\" \"F.Cu\")"),
            ("(attr smd)", "(attr smd) (zone (net 1))"),
            ("(attr smd)", "(attr smd) (net_tie_pad_groups \"1,2\")"),
            ("(attr smd)", "(attr smd) (path \"/schematic-source\")"),
            ("(attr smd)", "(attr smd) (unknown_semantics (nested 1))"),
        ] {
            blocked(&FIXTURE.replace(from, to));
        }
        blocked(&FIXTURE.replace("(layer \"F.Cu\")", ""));
    }
    #[test]
    fn malformed_ids_names_fields_and_exact_decimal_inputs_are_rejected_or_blocked() {
        for (from, to) in [
            ("70000000-0000-4000-8000-000000000001", "not-a-uuid"),
            ("70000000-0000-4000-8000-000000000001", "00000000-0000-0000-0000-000000000000"),
            ("70000000-0000-4000-8000-000000000001", "60000000-0000-4000-8000-000000000001"),
            ("(version 20241229)", "(version 20241229) (version 20241229)"),
            ("(version 20241229)", "(version 20990101)"),
            ("OriginalRect2", " invalid "),
            (
                "(uuid \"70000000-0000-4000-8000-000000000001\")",
                "(uuid \"70000000-0000-4000-8000-000000000001\") (tstamp \"71000000-0000-4000-8000-000000000001\")",
            ),
        ] {
            assert!(parse(&FIXTURE.replace(from, to)).is_err(), "{to}");
        }
        for value in ["0.0000001", "1e-6", "NaN", "inf", "10000.000001", "9223372036854775807", "--1"] {
            blocked(&FIXTURE.replace("0.8 1.2", &format!("{value} 1.2")));
        }
        for value in ["0", "-0.1"] {
            blocked(&FIXTURE.replace("0.8 1.2", &format!("{value} 1.2")));
        }
        assert!(parse(&(FIXTURE.to_owned() + " trailing")).is_err());
        assert!(parse(&FIXTURE.replace("OriginalRect2", "Original\\q")).is_err());
    }
    #[test]
    fn all_source_and_parser_work_limits_precede_unbounded_native_collection() {
        assert!(parse(&" ".repeat(MAX_SOURCE_BYTES + 1)).unwrap_err().0.contains("1 MiB"));
        assert!(parse(&format!("(footprint \"{}\")", "x".repeat(4097))).unwrap_err().0.contains("4096"));
        assert!(parse(&format!("{}{}", "(x ".repeat(34), ")".repeat(34))).unwrap_err().0.contains("depth"));
        let pads = "(pad \"1\" smd rect (at 0 0) (size 1 1) (layers \"F.Cu\"))".repeat(1025);
        assert!(parse(&format!("(footprint \"Original\" (version 20241229) (layer \"F.Cu\") {pads})")).unwrap_err().0.contains("1024"));
        assert!(parse(&format!("(footprint \"Original\" {})", "(x)".repeat(20000))).unwrap_err().0.contains("20000"));
        assert!(parse(&format!("(footprint \"Original\" {})", "x ".repeat(100000))).unwrap_err().0.contains("100000"));
    }
    #[test]
    fn unknown_long_tokens_retain_exact_provenance_without_diagnostic_amplification() {
        let token = "x".repeat(4096);
        let source = FIXTURE.replacen("(attr smd)", &format!("(attr smd) ({token} {})", "(child 1)".repeat(1000)), 1);
        let parsed = parse(&source).unwrap();
        assert!(parsed.footprint.is_none());
        assert!(parsed.report.items.iter().any(|i| i.token == token));
        assert!(parsed.report.items.iter().map(|i| i.reason.len()).sum::<usize>() < 512 * 1024);
    }
}

#[cfg(test)]
mod omitted_metadata_tests {
    use super::*;
    #[test]
    fn recognized_ignored_fields_and_art_cannot_hide_unknown_semantic_lists() {
        let source = include_str!("../../../fixtures/import/original-rect-smd.kicad_mod");
        for field in ["pinfunction", "pintype"] {
            let valid = source.replacen("(size 0.8 1.2)", &format!("(size 0.8 1.2) ({field} \"VIN\")"), 1);
            assert!(parse(&valid).unwrap().report.can_import());
            for injected in [format!("({field} \"VIN\" (zone (net 1)))"), format!("({field} \"VIN\") ({field} \"GND\")")] {
                let invalid = source.replacen("(size 0.8 1.2)", &format!("(size 0.8 1.2) {injected}"), 1);
                assert!(!parse(&invalid).unwrap().report.can_import());
            }
        }
        for (from, to) in [("(effects (font", "(effects (unknown 1) (font"), ("(offset (xyz 0 0 0))", "(zone (net 1))")] {
            assert!(!parse(&source.replace(from, to)).unwrap().report.can_import());
        }
    }
}
