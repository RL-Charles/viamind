//! Read-only batch DRC rules/results. Edits make reports visibly stale.
use crate::{ViamindApp, actions::UiAction};
use egui::Ui;
use serde_json::json;
use viamind_drc::{DrcReport, ViolationKind};
use viamind_engine::{AutoDrcConfig, AutoDrcPhase, EventCursor};

pub struct DrcUi {
    open: bool,
    copper: String,
    edge: String,
    drill: String,
    annular: String,
    holes: String,
    hole_copper: String,
    limit: usize,
    report: Option<DrcReport>,
    source_cursor: Option<EventCursor>,
    design_changes: u64,
    message: Option<String>,
    auto_synced: Option<AutoDrcConfig>,
    debounce_ms: u64,
    max_objects: usize,
}
impl Default for DrcUi {
    fn default() -> Self {
        Self {
            open: false,
            copper: "0.2".into(),
            edge: "0.25".into(),
            drill: "0.2".into(),
            annular: "0.1".into(),
            holes: "0.25".into(),
            hole_copper: "0.2".into(),
            limit: 1000,
            report: None,
            source_cursor: None,
            design_changes: 0,
            message: None,
            auto_synced: None,
            debounce_ms: 20,
            max_objects: 5000,
        }
    }
}
impl DrcUi {
    fn rules_input(&self) -> Result<serde_json::Value, String> {
        let parse = |text: &str| text.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| "Clearances need finite numbers in mm".to_owned());
        Ok(
            json!({"copper_clearance_mm":parse(&self.copper)?,"edge_clearance_mm":parse(&self.edge)?,"max_violations":self.limit,"minimum_via_drill_mm":parse(&self.drill)?,"minimum_annular_width_mm":parse(&self.annular)?,"hole_clearance_mm":parse(&self.holes)?,"hole_to_copper_clearance_mm":parse(&self.hole_copper)?}),
        )
    }
    fn sync_auto(&mut self, config: &AutoDrcConfig) {
        if self.auto_synced.as_ref() == Some(config) {
            return;
        }
        self.copper = config.rules.copper_clearance_mm.to_string();
        self.edge = config.rules.edge_clearance_mm.to_string();
        self.drill = config.rules.minimum_via_drill_mm.to_string();
        self.annular = config.rules.minimum_annular_width_mm.to_string();
        self.holes = config.rules.hole_clearance_mm.to_string();
        self.hole_copper = config.rules.hole_to_copper_clearance_mm.to_string();
        self.limit = config.rules.max_violations;
        self.debounce_ms = config.debounce_ms;
        self.max_objects = config.max_objects;
        self.auto_synced = Some(config.clone());
    }
}
impl ViamindApp {
    pub fn begin_drc(&mut self) {
        if self.design.is_some() {
            self.drc.open = true;
        }
    }
    pub fn run_design_drc(&mut self) {
        let Some(design) = &self.design else { return };
        let result = (|| {
            let input = self.drc.rules_input()?;
            let value = UiAction::RunDrc.run(&design.engine, input).map_err(|e| e.to_string())?;
            let report = serde_json::from_value(value["report"].clone()).map_err(|e| format!("Could not read DRC report: {e}"))?;
            let cursor = serde_json::from_value(value["source_cursor"].clone()).map_err(|e| format!("Could not read DRC cursor: {e}"))?;
            Ok::<_, String>((report, cursor))
        })();
        match result {
            Ok((report, cursor)) => {
                self.drc.report = Some(report);
                self.drc.source_cursor = Some(cursor);
                self.drc.design_changes = design.change_count;
                self.drc.message = None;
            }
            Err(error) => self.drc.message = Some(error),
        }
    }
    pub fn configure_design_auto_drc(&mut self, enabled: bool) {
        let Some(design) = &self.design else { return };
        if !enabled {
            let mut config = design.engine.auto_drc_status().config;
            config.enabled = false;
            self.drc.message =
                UiAction::ConfigureAutoDrc.run(&design.engine, serde_json::to_value(config).expect("valid config")).err().map(|e| e.to_string());
            self.drc.report = None;
            return;
        }
        let result = self.drc.rules_input().and_then(|rules| {
            UiAction::ConfigureAutoDrc
                .run(&design.engine, json!({"enabled":enabled,"rules":rules,"debounce_ms":self.drc.debounce_ms,"max_objects":self.drc.max_objects}))
                .map_err(|e| e.to_string())
        });
        self.drc.message = result.err();
    }
    pub(crate) fn drc_window(&mut self, ui: &mut Ui) {
        if !self.drc.open {
            return;
        }
        let Some(design) = &self.design else {
            self.drc.open = false;
            return;
        };
        let automatic = design.engine.auto_drc_status();
        self.drc.sync_auto(&automatic.config);
        let auto_enabled = automatic.config.enabled;
        if auto_enabled {
            self.drc.report = automatic.report.clone();
            self.drc.source_cursor = automatic.source_cursor;
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(20));
        }
        let stale = if auto_enabled {
            automatic.phase != AutoDrcPhase::Ready
        } else {
            self.drc.report.is_some() && self.drc.design_changes != design.change_count
        };
        let engine = design.engine.clone();
        let mut run = false;
        let mut configure = None;
        let mut select = None;
        egui::Window::new("Design rule check").open(&mut self.drc.open).default_width(570.0).default_height(600.0).show(ui.ctx(), |ui| {
            egui::ScrollArea::vertical().id_salt("drc_contents").show(ui, |ui| {
                ui.label("Scoped checks: SMD pads, straight tracks, through vias and polygon board boundaries.");
                ui.label("This report covers the listed checks. Additional fabrication checks remain.");
                ui.horizontal(|ui| {
                    let copper = ui.label("Copper clearance (mm)");
                    ui.add(egui::TextEdit::singleline(&mut self.drc.copper).desired_width(70.0)).labelled_by(copper.id);
                    let edge = ui.label("Board edge clearance (mm)");
                    ui.add(egui::TextEdit::singleline(&mut self.drc.edge).desired_width(70.0)).labelled_by(edge.id);
                });
                for (label, value) in [
                    ("Minimum via drill (mm)", &mut self.drc.drill),
                    ("Minimum annular width (mm)", &mut self.drc.annular),
                    ("Hole spacing (mm)", &mut self.drc.holes),
                    ("Foreign hole to copper (mm)", &mut self.drc.hole_copper),
                ] {
                    ui.horizontal(|ui| {
                        let caption = ui.label(label);
                        ui.add(egui::TextEdit::singleline(value).desired_width(70.0)).labelled_by(caption.id);
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Maximum issues");
                    ui.add(egui::DragValue::new(&mut self.drc.limit).range(1..=10000));
                    run = ui.add_enabled(!auto_enabled, egui::Button::new("Run DRC")).clicked();
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(if auto_enabled { "Disable automatic DRC" } else { "Enable automatic DRC" }).clicked() {
                        configure = Some(!auto_enabled);
                    }
                    if auto_enabled && ui.button("Apply automatic rules").clicked() {
                        configure = Some(true);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Debounce (ms)");
                    ui.add(egui::DragValue::new(&mut self.drc.debounce_ms).range(0..=2000));
                    ui.label("Snapshot object limit");
                    ui.add(egui::DragValue::new(&mut self.drc.max_objects).range(1..=10000));
                });
                if auto_enabled {
                    ui.label(format!("Automatic DRC: {:?} · generation {}", automatic.phase, automatic.generation));
                    ui.label(format!(
                        "Bounded snapshot: {} objects, {} text bytes. Scoped checker limits still apply.",
                        automatic.config.max_objects, automatic.max_snapshot_text_bytes
                    ));
                    if let Some(message) = &automatic.message {
                        ui.label(message);
                    }
                    if let (Some(check), Some(total)) = (automatic.check_ms, automatic.request_to_publication_ms) {
                        ui.label(format!(
                            "Snapshot {:.2} ms · worker check {check:.2} ms · request to publication {total:.2} ms (excludes desktop presentation)",
                            automatic.snapshot_capture_ms.unwrap_or_default()
                        ));
                    }
                }
                if let Some(message) = &self.drc.message {
                    ui.label(message);
                }
                if stale {
                    ui.label(if auto_enabled {
                        "Waiting for current automatic results; issue selection is unavailable."
                    } else {
                        "Results are stale: the design changed. Run DRC again."
                    });
                }
                if let Some(report) = &self.drc.report {
                    let coverage = &report.coverage;
                    ui.label(format!(
                        "{} scoped violations · {} explicit pads · {} unassigned pads · {} excluded parts",
                        report.violations.len(),
                        coverage.checked_pads,
                        coverage.unassigned_pads,
                        coverage.excluded_part_count
                    ));
                    ui.label(format!(
                        "{} tracks · {} through vias · {} drilled holes",
                        coverage.checked_tracks, coverage.checked_vias, coverage.checked_holes
                    ));
                    if let Some(reason) = &report.incomplete_reason {
                        ui.label(format!("Incomplete: {reason}"));
                    }
                    if !coverage.explicit_outline_checked {
                        ui.label("Board boundary checks excluded: no explicit outline.");
                    }
                    ui.collapsing("Coverage and exclusions", |ui| {
                        if coverage.excluded_details_truncated {
                            ui.label(format!("Showing first {} of {} excluded parts", coverage.excluded_parts.len(), coverage.excluded_part_count));
                        }
                        for part in &coverage.excluded_parts {
                            ui.label(format!("Excluded {} ({}): {}", part.designator, part.part_id, part.reason));
                        }
                        for check in &coverage.checks_not_performed {
                            ui.label(format!("Not checked: {check}"));
                        }
                        if let Some(cursor) = self.drc.source_cursor {
                            ui.monospace(format!("Snapshot {} / {}", cursor.stream_id, cursor.sequence));
                        }
                    });
                    for (i, issue) in report.violations.iter().enumerate() {
                        ui.separator();
                        let title = match issue.kind {
                            ViolationKind::CopperClearance => "Copper clearance",
                            ViolationKind::OutsideSubstrate => "Outside board / inside cutout",
                            ViolationKind::BoardEdgeClearance => "Board edge clearance",
                            ViolationKind::MinimumViaDrill => "Minimum via drill",
                            ViolationKind::MinimumAnnularWidth => "Minimum annular width",
                            ViolationKind::HoleClearance => "Drilled hole spacing",
                            ViolationKind::HoleToCopperClearance => "Foreign hole to copper clearance",
                        };
                        ui.label(format!("{title}: {}", issue.message));
                        ui.label(format!("Measured {:.6} mm; required {:.6} mm", issue.measured_clearance.to_mm(), issue.required_clearance.to_mm()));
                        for object in &issue.objects {
                            ui.monospace(object.label());
                        }
                        if ui.add_enabled(!stale, egui::Button::new(format!("Select issue {}", i + 1))).clicked() {
                            select =
                                Some(issue.objects.iter().map(viamind_drc::CopperReference::selection_id).collect::<std::collections::BTreeSet<_>>());
                        }
                    }
                }
            });
        });
        if let Some(enabled) = configure {
            self.configure_design_auto_drc(enabled);
        }
        if run {
            self.run_design_drc();
        }
        if let Some(ids) = select
            && let Err(error) = UiAction::SetSelection.run(&engine, json!({"ids":ids}))
        {
            self.drc.message = Some(error.to_string());
        }
    }
}
