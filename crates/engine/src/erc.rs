//! Bounded native pin-policy checks over one exact schematic graph snapshot.
use crate::{CommandError, EngineHandle, EventCursor, SchematicConnectivityCoverage, SchematicNet, SchematicNetMember};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use viamind_doc::{ObjectId, schematic::PinType};

const MAX_PIN_PAIRS: usize = 250_000;
const TYPES: [PinType; 11] = [
    PinType::Unspecified,
    PinType::Passive,
    PinType::Input,
    PinType::Output,
    PinType::Bidirectional,
    PinType::TriState,
    PinType::OpenCollector,
    PinType::OpenEmitter,
    PinType::PowerInput,
    PinType::PowerOutput,
    PinType::NoConnect,
];
fn index(t: PinType) -> usize {
    TYPES.iter().position(|&v| v == t).expect("all native pin types")
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErcSeverity {
    Allow,
    Warning,
    #[default]
    Error,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PinCompatibilityRule {
    pub a: PinType,
    pub b: PinType,
    pub severity: ErcSeverity,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ErcInput {
    /// Overrides for unordered pin-type pairs; duplicate/reversed pairs reject.
    pub compatibility: Vec<PinCompatibilityRule>,
    pub unconnected_input: ErcSeverity,
    pub undriven_input: ErcSeverity,
    pub missing_power: ErcSeverity,
    pub no_connect_attached: ErcSeverity,
    #[schemars(range(min = 1, max = 10000))]
    pub max_violations: usize,
}
impl Default for ErcInput {
    fn default() -> Self {
        Self {
            compatibility: Vec::new(),
            unconnected_input: ErcSeverity::Error,
            undriven_input: ErcSeverity::Error,
            missing_power: ErcSeverity::Error,
            no_connect_attached: ErcSeverity::Warning,
            max_violations: 1000,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErcCheck {
    UnconnectedInput,
    UndrivenInput,
    MissingPower,
    NoConnectAttached,
    PinCompatibility,
}
impl ErcCheck {
    fn name(self) -> &'static str {
        match self {
            Self::UnconnectedInput => "unconnected_input",
            Self::UndrivenInput => "undriven_input",
            Self::MissingPower => "missing_power",
            Self::NoConnectAttached => "no_connect_attached",
            Self::PinCompatibility => "pin_compatibility",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ErcViolation {
    pub id: String,
    pub check: ErcCheck,
    pub severity: ErcSeverity,
    pub net_id: String,
    pub net_name: Option<String>,
    pub pins: Vec<SchematicNetMember>,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ErcCoverage {
    pub full_erc: bool,
    /// Coverage of the graph stage; ERC is assessed by the following pin-policy stage.
    pub connectivity: SchematicConnectivityCoverage,
    pub visited_nets: usize,
    pub evaluated_pins: usize,
    pub unspecified_pins: usize,
    pub tested_pin_pairs: usize,
    pub max_pin_pairs: usize,
    pub checks_not_performed: Vec<&'static str>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ErcReport {
    pub project_id: ObjectId,
    pub source_cursor: EventCursor,
    /// Complete only for the selected supported checks, never all ERC.
    pub complete: bool,
    pub stop_reason: Option<&'static str>,
    pub policy: ErcInput,
    pub effective_matrix: Vec<PinCompatibilityRule>,
    pub coverage: ErcCoverage,
    pub violations: Vec<ErcViolation>,
}
fn strong(t: PinType) -> bool {
    matches!(t, PinType::Output | PinType::PowerOutput)
}
fn driver(t: PinType) -> bool {
    matches!(
        t,
        PinType::Output
            | PinType::PowerOutput
            | PinType::Bidirectional
            | PinType::TriState
            | PinType::OpenCollector
            | PinType::OpenEmitter
            | PinType::Passive
    )
}
fn default_pair(a: PinType, b: PinType) -> ErcSeverity {
    if a == PinType::NoConnect || b == PinType::NoConnect {
        ErcSeverity::Error
    } else if a == PinType::Unspecified || b == PinType::Unspecified {
        ErcSeverity::Warning
    } else if strong(a) && strong(b) {
        ErcSeverity::Error
    } else {
        ErcSeverity::Allow
    }
}
impl ErcInput {
    /// Effective symmetric native policy, ordered by the supported unordered pin pairs.
    pub fn effective_matrix(&self) -> Result<Vec<PinCompatibilityRule>, CommandError> {
        let matrix = self.matrix()?;
        let mut rules = Vec::with_capacity(66);
        for (a, &kind_a) in TYPES.iter().enumerate() {
            for (b, &kind_b) in TYPES.iter().enumerate().skip(a) {
                rules.push(PinCompatibilityRule { a: kind_a, b: kind_b, severity: matrix[a][b] });
            }
        }
        Ok(rules)
    }
    fn matrix(&self) -> Result<[[ErcSeverity; 11]; 11], CommandError> {
        if !(1..=10_000).contains(&self.max_violations) {
            return Err(CommandError::invalid("max_violations", "must be within 1..10000"));
        }
        if self.compatibility.len() > 66 {
            return Err(CommandError::invalid("compatibility", "at most 66 unordered pairs"));
        }
        let mut matrix = [[ErcSeverity::Allow; 11]; 11];
        for (i, &a) in TYPES.iter().enumerate() {
            for (j, &b) in TYPES.iter().enumerate() {
                matrix[i][j] = default_pair(a, b);
            }
        }
        let mut seen = BTreeSet::new();
        for rule in &self.compatibility {
            let (a, b) = (index(rule.a), index(rule.b));
            if !seen.insert((a.min(b), a.max(b))) {
                return Err(CommandError::invalid("compatibility", "duplicate or reversed duplicate unordered pair"));
            }
            matrix[a][b] = rule.severity;
            matrix[b][a] = rule.severity;
        }
        Ok(matrix)
    }
}
impl EngineHandle {
    pub fn run_erc(&self, input: &ErcInput) -> Result<ErcReport, CommandError> {
        let matrix = input.matrix()?;
        let graph = self.all_schematic_connectivity()?;
        let effective_matrix = input.effective_matrix()?;
        let unspecified_pins = graph.nets.iter().flat_map(|n| &n.members).filter(|m| m.electrical == Some(PinType::Unspecified)).count();
        let mut report = ErcReport {
            project_id: graph.project_id,
            source_cursor: graph.source_cursor,
            complete: true,
            stop_reason: None,
            policy: input.clone(),
            effective_matrix,
            coverage: ErcCoverage {
                full_erc: false,
                connectivity: graph.coverage,
                visited_nets: 0,
                evaluated_pins: 0,
                unspecified_pins,
                tested_pin_pairs: 0,
                max_pin_pairs: MAX_PIN_PAIRS,
                checks_not_performed: vec![
                    "hierarchy, buses and hidden power pins",
                    "no-connect markers and power flags absent from the native model",
                    "multi-unit/shared pins and simulation",
                    "board annotation, net classes and off-grid checks",
                    "wire/label diagnostics and electrical correctness beyond selected pin policy",
                ],
            },
            violations: Vec::new(),
        };
        check_nets(&graph.nets, input, &matrix, &mut report, MAX_PIN_PAIRS);
        Ok(report)
    }
}
fn add(report: &mut ErcReport, net: &SchematicNet, pins: Vec<SchematicNetMember>, check: ErcCheck, severity: ErcSeverity, message: &str) -> bool {
    if severity == ErcSeverity::Allow {
        return true;
    }
    if report.violations.len() >= report.policy.max_violations {
        report.complete = false;
        report.stop_reason = Some("violation_limit");
        return false;
    }
    let ids = pins.iter().map(|p| p.id.to_string()).collect::<Vec<_>>().join(":");
    report.violations.push(ErcViolation {
        id: format!("{}:{}:{ids}", check.name(), net.id),
        check,
        severity,
        net_id: net.id.clone(),
        net_name: net.canonical_name.clone(),
        pins,
        message: message.into(),
    });
    true
}
fn check_nets(nets: &[SchematicNet], input: &ErcInput, matrix: &[[ErcSeverity; 11]; 11], report: &mut ErcReport, pair_limit: usize) {
    for net in nets {
        report.coverage.visited_nets += 1;
        let mut pins: Vec<_> = net.members.iter().filter(|m| m.electrical.is_some()).collect();
        pins.sort_by_key(|p| p.id);
        let has_driver = pins.iter().any(|p| driver(p.electrical.expect("typed pins")));
        let has_power = pins.iter().any(|p| p.electrical == Some(PinType::PowerOutput));
        for pin in &pins {
            report.coverage.evaluated_pins += 1;
            let t = pin.electrical.expect("typed pins");
            let checks = [
                (
                    t == PinType::Input && pins.len() == 1,
                    ErcCheck::UnconnectedInput,
                    input.unconnected_input,
                    "Input has no other pin in its net; connect a source or disable this policy check",
                ),
                (
                    t == PinType::Input && !has_driver,
                    ErcCheck::UndrivenInput,
                    input.undriven_input,
                    "Input has no supported signal-driving pin in its net",
                ),
                (
                    t == PinType::PowerInput && !has_power,
                    ErcCheck::MissingPower,
                    input.missing_power,
                    "Power input has no power_output pin; a power label is not a source",
                ),
                (
                    t == PinType::NoConnect && net.members.len() > 1,
                    ErcCheck::NoConnectAttached,
                    input.no_connect_attached,
                    "No-connect pin is attached to another graph member",
                ),
            ];
            for (condition, check, severity, message) in checks {
                if condition && !add(report, net, vec![(*pin).clone()], check, severity, message) {
                    return;
                }
            }
        }
        for (a, pin_a) in pins.iter().enumerate() {
            for pin_b in pins.iter().skip(a + 1) {
                if report.coverage.tested_pin_pairs >= pair_limit {
                    report.complete = false;
                    report.stop_reason = Some("pin_pair_limit");
                    return;
                }
                report.coverage.tested_pin_pairs += 1;
                let severity = matrix[index(pin_a.electrical.expect("typed pins"))][index(pin_b.electrical.expect("typed pins"))];
                if severity != ErcSeverity::Allow
                    && !add(
                        report,
                        net,
                        vec![(*pin_a).clone(), (*pin_b).clone()],
                        ErcCheck::PinCompatibility,
                        severity,
                        "Connected pin types violate the selected compatibility matrix",
                    )
                {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use viamind_doc::{
        Project, Sheet,
        schematic::{Coordinate, LabelScope, SchematicContent, SchematicJunction, SchematicLabel, SchematicSymbol, SchematicWire, SymbolPin},
    };
    use viamind_geom::Nm;
    fn at(x: i64, y: i64) -> Coordinate {
        Coordinate { x: Nm(x), y: Nm(y) }
    }
    fn pins(types: &[PinType], same_point: bool) -> EngineHandle {
        let mut p = Project::new("ERC");
        let mut s = Sheet::new("S");
        let mut c = SchematicContent::default();
        for (i, &kind) in types.iter().enumerate() {
            let id = ObjectId::new();
            c.symbols.insert(
                id,
                SchematicSymbol {
                    id,
                    designator: format!("U{i}"),
                    value: String::new(),
                    footprint: None,
                    position: at(if same_point { 0 } else { i as i64 * 100 }, 0),
                    rotation_quarters: 0,
                    mirrored: false,
                    width: Nm(10),
                    height: Nm(10),
                    pins: vec![SymbolPin { id: ObjectId::new(), number: "1".into(), name: String::new(), position: at(0, 0), electrical: kind }],
                },
            );
        }
        s.content = Some(c);
        p.schematic.sheets.insert(s.id, s);
        EngineHandle::new(p)
    }
    #[test]
    fn matrix_is_complete_symmetric_deliberate_and_overrides_are_unordered() {
        let input = ErcInput::default();
        let m = input.matrix().unwrap();
        for (i, &a) in TYPES.iter().enumerate() {
            for (j, &b) in TYPES.iter().enumerate() {
                assert_eq!(m[i][j], m[j][i]);
                assert_eq!(m[i][j], default_pair(a, b));
            }
        }
        assert_eq!(m[index(PinType::Output)][index(PinType::Output)], ErcSeverity::Error);
        assert_eq!(m[index(PinType::PowerOutput)][index(PinType::Output)], ErcSeverity::Error);
        assert_eq!(m[index(PinType::Unspecified)][index(PinType::NoConnect)], ErcSeverity::Error);
        assert_eq!(m[index(PinType::Input)][index(PinType::Unspecified)], ErcSeverity::Warning);
        assert_eq!(m[index(PinType::OpenCollector)][index(PinType::OpenCollector)], ErcSeverity::Allow);
        let e = pins(&[PinType::Output, PinType::Output], true);
        for severity in [ErcSeverity::Allow, ErcSeverity::Warning, ErcSeverity::Error] {
            let input =
                ErcInput { compatibility: vec![PinCompatibilityRule { a: PinType::Output, b: PinType::Output, severity }], ..ErcInput::default() };
            let r = e.run_erc(&input).unwrap();
            assert_eq!(r.effective_matrix.len(), 66);
            assert_eq!(r.violations.len(), usize::from(severity != ErcSeverity::Allow));
            if let Some(v) = r.violations.first() {
                assert_eq!(v.severity, severity);
                assert_eq!(v.pins.len(), 2);
            }
        }
        let input = ErcInput {
            compatibility: vec![
                PinCompatibilityRule { a: PinType::Input, b: PinType::Output, severity: ErcSeverity::Allow },
                PinCompatibilityRule { a: PinType::Output, b: PinType::Input, severity: ErcSeverity::Warning },
            ],
            ..ErcInput::default()
        };
        assert!(input.matrix().unwrap_err().to_string().contains("duplicate"));
        for limit in [0, 10_001, usize::MAX] {
            assert!(e.run_erc(&ErcInput { max_violations: limit, ..ErcInput::default() }).is_err());
        }
    }
    #[test]
    fn isolated_inputs_short_outputs_missing_power_and_passive_signal_policy() {
        let isolated = pins(&[PinType::Input], false).run_erc(&ErcInput::default()).unwrap();
        assert_eq!(isolated.violations.iter().map(|v| v.check).collect::<Vec<_>>(), [ErcCheck::UnconnectedInput, ErcCheck::UndrivenInput]);
        for driver in [
            PinType::Output,
            PinType::Bidirectional,
            PinType::TriState,
            PinType::OpenCollector,
            PinType::OpenEmitter,
            PinType::PowerOutput,
            PinType::Passive,
        ] {
            assert!(pins(&[PinType::Input, driver], true).run_erc(&ErcInput::default()).unwrap().violations.is_empty());
        }
        let e = pins(&[PinType::PowerInput, PinType::Passive], true);
        let r = e.run_erc(&ErcInput::default()).unwrap();
        assert_eq!(r.violations.len(), 1);
        assert_eq!(r.violations[0].check, ErcCheck::MissingPower);
        assert!(pins(&[PinType::PowerInput, PinType::PowerOutput], true).run_erc(&ErcInput::default()).unwrap().violations.is_empty());
        let r = pins(&[PinType::Output, PinType::Output], true).run_erc(&ErcInput::default()).unwrap();
        assert_eq!(r.violations[0].check, ErcCheck::PinCompatibility);
        assert!(!r.coverage.full_erc);
        assert!(r.complete);
        assert!(!r.coverage.checks_not_performed.is_empty());
    }
    #[test]
    fn explicit_power_label_never_supplies_power_and_no_connect_attachment_is_checked() {
        let e = pins(&[PinType::PowerInput], true);
        let mut p = e.read(Clone::clone);
        let c = p.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap();
        let id = ObjectId::new();
        c.labels.insert(id, SchematicLabel { id, position: at(0, 0), name: "VCC".into(), scope: LabelScope::Power });
        let r = EngineHandle::new(p).run_erc(&ErcInput::default()).unwrap();
        assert_eq!(r.violations[0].check, ErcCheck::MissingPower);
        let e = pins(&[PinType::NoConnect], true);
        let mut p = e.read(Clone::clone);
        let c = p.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap();
        let id = ObjectId::new();
        c.wires.insert(id, SchematicWire { id, start: at(0, 0), end: at(10, 0) });
        let e = EngineHandle::new(p);
        let before = e.session_state();
        let r = e.run_erc(&ErcInput::default()).unwrap();
        assert_eq!(r.violations[0].check, ErcCheck::NoConnectAttached);
        assert_eq!(r.violations[0].severity, ErcSeverity::Warning);
        assert_eq!(r, e.run_erc(&ErcInput::default()).unwrap());
        assert_eq!(before, e.session_state());
        assert!(!e.can_undo());
        assert!(e.run_erc(&ErcInput { no_connect_attached: ErcSeverity::Allow, ..ErcInput::default() }).unwrap().violations.is_empty());
    }
    #[test]
    fn crossing_requires_junction_and_dangling_wire_does_not_drive_input() {
        let e = pins(&[PinType::Input, PinType::Output], false);
        let mut p = e.read(Clone::clone);
        let c = p.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap();
        // Move output to the vertical segment endpoint, leaving a pure interior crossing.
        let out = c.symbols.values_mut().find(|s| s.pins[0].electrical == PinType::Output).unwrap();
        out.position = at(5, 10);
        for (a, b) in [(at(0, 0), at(10, 0)), (at(5, -10), at(5, 10))] {
            let id = ObjectId::new();
            c.wires.insert(id, SchematicWire { id, start: a, end: b });
        }
        assert_eq!(EngineHandle::new(p.clone()).run_erc(&ErcInput::default()).unwrap().violations.len(), 2);
        let c = p.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap();
        let id = ObjectId::new();
        c.junctions.insert(id, SchematicJunction { id, position: at(5, 0) });
        assert!(EngineHandle::new(p).run_erc(&ErcInput::default()).unwrap().violations.is_empty());
    }
    #[test]
    fn truncation_is_an_honest_prefix_and_exact_threshold_can_be_complete() {
        let e = pins(&[PinType::Input, PinType::Input], false);
        let full = e.run_erc(&ErcInput::default()).unwrap();
        let limited = e.run_erc(&ErcInput { max_violations: 1, ..ErcInput::default() }).unwrap();
        assert!(!limited.complete);
        assert_eq!(limited.stop_reason, Some("violation_limit"));
        assert_eq!(limited.violations, full.violations[..1]);
        let e = pins(&[PinType::Output, PinType::Output], true);
        assert!(e.run_erc(&ErcInput { max_violations: 1, ..ErcInput::default() }).unwrap().complete);
        let e = pins(&[PinType::Unspecified], true);
        let r = e.run_erc(&ErcInput::default()).unwrap();
        assert_eq!(r.coverage.unspecified_pins, 1);
        let e = pins(&vec![PinType::Passive; 710], true);
        let r = e.run_erc(&ErcInput::default()).unwrap();
        assert!(!r.complete);
        assert_eq!(r.stop_reason, Some("pin_pair_limit"));
        assert_eq!(r.coverage.tested_pin_pairs, MAX_PIN_PAIRS);
        assert!(r.violations.is_empty());
    }
    #[test]
    fn invalid_graph_rejects_instead_of_a_clean_erc_report() {
        let e = pins(&[PinType::Input], true);
        let mut p = e.read(Clone::clone);
        let c = p.schematic.sheets.values_mut().next().unwrap().content.as_mut().unwrap();
        for (a, b) in [(at(-10, 5), at(10, 5)), (at(0, -10), at(0, 10))] {
            let id = ObjectId::new();
            c.wires.insert(id, SchematicWire { id, start: a, end: b });
        }
        let id = ObjectId::new();
        c.labels.insert(id, SchematicLabel { id, position: at(0, 5), name: "ambiguous".into(), scope: LabelScope::Local });
        assert!(EngineHandle::new(p).run_erc(&ErcInput::default()).unwrap_err().to_string().contains("ambiguous label"));
    }
}
