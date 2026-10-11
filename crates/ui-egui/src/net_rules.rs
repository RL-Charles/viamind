//! Local exact-decimal policy editor. Only the shared guarded command mutates a board.
use crate::{ViamindApp, actions::UiAction};
use egui::{Color32, Ui};
use viamind_doc::ObjectId;
use viamind_engine::{EventCursor, NamedNetRuleRow, SetNamedNetRules};
use viamind_geom::Nm;

struct Row {
    net: String,
    clearance: String,
    width: String,
}
struct Draft {
    project: ObjectId,
    cursor: EventCursor,
    rows: Vec<Row>,
    message: Option<String>,
}
#[derive(Default)]
pub struct NetRulesUi {
    draft: Option<Draft>,
}
impl NetRulesUi {
    pub fn editing(&self) -> bool {
        self.draft.is_some()
    }
    pub fn message(&self) -> Option<&str> {
        self.draft.as_ref().and_then(|d| d.message.as_deref())
    }
    pub(crate) fn cancel(&mut self) {
        self.draft = None;
    }
}
/// Decimal millimetres convert exactly to integer Nm; unsupported precision refuses.
pub fn parse_minimum_mm(text: &str) -> Result<Nm, String> {
    if text.is_empty() || text.len() > 32 || text.trim() != text {
        return Err("Use a nonnegative exact decimal in mm with at most six fractional digits".into());
    }
    let mut pieces = text.split('.');
    let whole = pieces.next().unwrap();
    let fractional = pieces.next().unwrap_or("");
    if pieces.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || fractional.len() > 6
        || !fractional.bytes().all(|c| c.is_ascii_digit())
    {
        return Err("Use a nonnegative exact decimal in mm with at most six fractional digits".into());
    }
    let whole = whole.parse::<i64>().map_err(|_| "Minimum is out of range".to_owned())?;
    let fraction = if fractional.is_empty() { 0 } else { fractional.parse::<i64>().map_err(|_| "Invalid fractional minimum".to_owned())? };
    let value = whole
        .checked_mul(1_000_000)
        .and_then(|w| w.checked_add(fraction * 10_i64.pow(6 - fractional.len() as u32)))
        .filter(|n| *n <= 100_000_000)
        .ok_or_else(|| "Minimum must be at most 100 mm".to_owned())?;
    Ok(Nm(value))
}
fn mm(value: Nm) -> String {
    format!("{}.{:06}", value.0 / 1_000_000, value.0 % 1_000_000)
}
impl ViamindApp {
    pub fn begin_named_net_rules(&mut self) {
        if self.schematic.active
            || self.files.busy()
            || self.parts.placing()
            || self.outlines.editing()
            || self.pads.editing()
            || self.copper.editing()
            || self.routing.editing()
            || self.track_cleanup.editing()
            || self.schematic_wires.active()
            || self.schematic_object_draft_active()
            || self.net_rules.editing()
        {
            return;
        }
        self.reload_named_net_rules();
    }
    fn reload_named_net_rules(&mut self) {
        let Some(design) = &self.design else { return };
        match UiAction::QueryNamedNetRules.run(&design.engine, serde_json::json!({})).and_then(|v| {
            serde_json::from_value::<viamind_engine::NamedNetRuleSnapshot>(v)
                .map_err(|e| viamind_engine::EngineError::InvalidInput { command: "query_named_net_rules".into(), message: e.to_string() })
        }) {
            Ok(snapshot) => {
                self.selection.cancel_gestures();
                self.net_rules.draft = Some(Draft {
                    project: snapshot.project_id,
                    cursor: snapshot.source_cursor,
                    rows: snapshot
                        .document_net_rules
                        .into_iter()
                        .map(|(net, r)| Row { net, clearance: mm(r.minimum_copper_clearance), width: mm(r.minimum_track_width) })
                        .collect(),
                    message: None,
                });
            }
            Err(error) => self.files.message = Some(error.to_string()),
        }
    }
    pub(crate) fn named_net_rules_window(&mut self, ui: &mut Ui) {
        if self.schematic.active || self.files.busy() {
            self.net_rules.cancel();
            return;
        }
        let Some(design) = &self.design else {
            self.net_rules.cancel();
            return;
        };
        let Some(draft) = &mut self.net_rules.draft else { return };
        let engine = design.engine.clone();
        let current = engine.event_cursor();
        let stale = current != draft.cursor;
        let (mut open, mut apply, mut cancel, mut reload) = (true, false, false, false);
        let mut remove = None;
        let content = ui.ctx().content_rect();
        egui::Window::new("Exact named-net minima").open(&mut open).default_width(470.).max_width((content.width()-24.).max(240.)).max_height((content.height()-24.).max(160.)).show(ui.ctx(),|ui| {
            egui::ScrollArea::vertical().max_height((content.height()-145.).clamp(60.,650.)).show(ui,|ui| {
                ui.label("Enforced document floors in mm. Copper pairs use the greater request baseline and both exact net minima. Width remains an explicit routing choice.");
                ui.label("No preferred sizes, automatic widening, patterns or net assignment. Existing copper may violate newly saved minima; run scoped DRC.");
                ui.label(format!("{} of 128 rules; empty restores legacy document policy",draft.rows.len()));
                for (i,row) in draft.rows.iter_mut().enumerate() {
                    ui.push_id(i,|ui| {
                        ui.separator();
                        ui.horizontal(|ui| {let label=ui.label(format!("Net {}",i+1));ui.add(egui::TextEdit::singleline(&mut row.net).char_limit(128).desired_width(150.)).labelled_by(label.id);if ui.button(format!("Remove rule {}",i+1)).clicked(){remove=Some(i);} });
                        ui.horizontal(|ui| {let label=ui.label(format!("Clearance {} (mm)",i+1));ui.add(egui::TextEdit::singleline(&mut row.clearance).char_limit(32).desired_width(100.)).labelled_by(label.id);});
                        ui.horizontal(|ui| {let label=ui.label(format!("Track width {} (mm)",i+1));ui.add(egui::TextEdit::singleline(&mut row.width).char_limit(32).desired_width(100.)).labelled_by(label.id);});
                    });
                }
                if ui.add_enabled(draft.rows.len()<128,egui::Button::new("Add exact net rule")).clicked() {draft.rows.push(Row {net:format!("N{}",draft.rows.len()+1),clearance:"0.2".into(),width:"0.25".into()});}
                if stale {ui.colored_label(Color32::YELLOW,"Document or selection changed. Reload rules before applying; local rows remain uncommitted.");}
                if let Some(message)=&draft.message {ui.colored_label(Color32::LIGHT_RED,message);}
            });
            ui.horizontal_wrapped(|ui| {
                apply=ui.add_enabled(!stale,egui::Button::new("Apply named-net rules")).clicked();
                reload=ui.button("Reload rules").clicked();
                cancel=ui.button("Cancel rules").clicked();
            });
        });
        if let Some(index) = remove {
            draft.rows.remove(index);
        }
        cancel |= !open || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel {
            self.net_rules.cancel();
            return;
        }
        if reload {
            self.reload_named_net_rules();
            return;
        }
        if apply {
            let input = (|| {
                let mut rows = Vec::with_capacity(draft.rows.len());
                for row in &draft.rows {
                    viamind_doc::net_rules::validate_net_rule_name(&row.net).map_err(str::to_owned)?;
                    rows.push(NamedNetRuleRow {
                        net: row.net.clone(),
                        minimum_copper_clearance: parse_minimum_mm(&row.clearance)?,
                        minimum_track_width: parse_minimum_mm(&row.width)?,
                    });
                }
                Ok::<_, String>(SetNamedNetRules { expected_project: draft.project, expected_cursor: draft.cursor, rules: rows })
            })();
            let result = input.and_then(|input| {
                UiAction::SetNamedNetRules
                    .run(&engine, serde_json::to_value(input).expect("bounded rule input serializes"))
                    .map_err(|e| e.to_string())
            });
            match result {
                Ok(_) => self.net_rules.cancel(),
                Err(error) => draft.message = Some(error),
            }
        }
    }
}

pub(crate) fn show_document_minima(ui: &mut Ui, rules: &viamind_doc::NamedNetRules) {
    if !rules.is_empty() {
        ui.collapsing(format!("{} persisted exact-net minima (pair maxima apply)", rules.len()), |ui| {
            for (net, r) in rules {
                ui.label(format!(
                    "{net}: copper floor {} mm; explicit track-width floor {} mm",
                    mm(r.minimum_copper_clearance),
                    mm(r.minimum_track_width)
                ));
            }
        });
    }
}
