//! Human review of bounded private changes; ghosts never enter hit testing.
use crate::{ViamindApp, actions::UiAction, scene::Item};
use egui::{Color32, Ui};
use serde_json::json;
use viamind_doc::{Board, ObjectId, Project};
use viamind_engine::{
    Delta, EngineHandle,
    pcb_branches::{PcbBranchDiff, PcbBranchInfo},
};

const ADDED: Color32 = Color32::from_rgba_premultiplied(30, 190, 85, 210);
const REMOVED: Color32 = Color32::from_rgba_premultiplied(210, 45, 55, 210);
struct GhostCache {
    id: ObjectId,
    revision: u64,
    items: Vec<(Item, Color32)>,
    details: Vec<String>,
}
#[derive(Default)]
pub struct PcbBranchesUi {
    open: bool,
    shown: bool,
    selected: Option<ObjectId>,
    branches: Vec<PcbBranchInfo>,
    observed_stream: Option<ObjectId>,
    controls: Vec<(egui::Rect, egui::Rect)>,
    cached: Option<GhostCache>,
    message: Option<String>,
}
impl PcbBranchesUi {
    pub fn open(&self) -> bool {
        self.open
    }
    pub fn visible(&self) -> bool {
        self.shown && self.cached.is_some()
    }
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    pub fn ghost_items(&self) -> usize {
        self.cached.as_ref().map_or(0, |cache| cache.items.len())
    }
    /// Actual current-pass review response bounds and their effective paint clips.
    pub fn control_rects(&self) -> &[(egui::Rect, egui::Rect)] {
        &self.controls
    }
    fn refresh(&mut self, engine: &EngineHandle) {
        let current = engine.event_cursor();
        if self.observed_stream.is_some_and(|stream| stream != current.stream_id) {
            self.clear();
        }
        self.observed_stream = Some(current.stream_id);
        match UiAction::ListPcbBranches.run(engine, json!({})).and_then(|v| {
            serde_json::from_value(v).map_err(|e| viamind_engine::EngineError::InvalidInput { command: "pcb_branch".into(), message: e.to_string() })
        }) {
            Ok(branches) => self.branches = branches,
            Err(error) => self.message = Some(error.to_string()),
        }
        if let Some(id) = self.selected {
            let Some(info) = self.branches.iter().find(|b| b.branch_id == id) else {
                self.clear();
                return;
            };
            let foreign = info.source_cursor.stream_id != current.stream_id || info.project_id != engine.read(|p| p.id);
            if foreign {
                self.shown = false;
            }
            if self.cached.as_ref().is_some_and(|cache| cache.id == id && cache.revision == info.revision) {
                return;
            }
            match UiAction::GetPcbBranchDiff.run(engine, json!({"branch_id":id,"offset":0,"limit":128})) {
                Ok(value) => match serde_json::from_value::<PcbBranchDiff>(value) {
                    Ok(diff) => {
                        let details = details(&diff.deltas);
                        self.cached = Some(GhostCache {
                            id,
                            revision: diff.branch.revision,
                            items: if foreign { Vec::new() } else { ghosts(diff.deltas) },
                            details,
                        });
                    }
                    Err(error) => {
                        self.cached = None;
                        self.message = Some(error.to_string());
                    }
                },
                Err(error) => {
                    self.cached = None;
                    self.message = Some(error.to_string());
                }
            }
        }
    }
    fn clear(&mut self) {
        self.shown = false;
        self.selected = None;
        self.cached = None;
    }
}
fn abbreviated(text: &str) -> String {
    let mut value: String = text.chars().take(80).collect();
    if text.chars().count() > 80 {
        value.push('…');
    }
    value
}
fn details(deltas: &[Delta]) -> Vec<String> {
    deltas
        .iter()
        .map(|delta| match delta {
            Delta::Part { id, before, after } => {
                let summary = |p: &viamind_doc::Part| {
                    format!(
                        "{} value={} footprint={} at({}, {})Nm rotation={}mdeg {:?} pads={}",
                        abbreviated(&p.designator),
                        abbreviated(&p.value),
                        abbreviated(&p.footprint),
                        p.position.x.0,
                        p.position.y.0,
                        p.rotation_mdeg,
                        p.side,
                        p.pads.as_ref().map_or(0, |pads| pads.0.len())
                    )
                };
                format!(
                    "Part {id}\nOld: {}\nNew: {}",
                    before.as_ref().map_or_else(|| "absent".into(), &summary),
                    after.as_ref().map_or_else(|| "absent".into(), summary)
                )
            }
            Delta::Track { id, before, after } => {
                let summary = |t: &viamind_doc::Track| {
                    format!(
                        "({}, {}) to({}, {})Nm width={}Nm {:?} net={}",
                        t.start.x.0,
                        t.start.y.0,
                        t.end.x.0,
                        t.end.y.0,
                        t.width.0,
                        t.layer,
                        t.net.as_deref().map_or_else(|| "unassigned".into(), abbreviated)
                    )
                };
                format!(
                    "Track {id}\nOld: {}\nNew: {}",
                    before.as_ref().map_or_else(|| "absent".into(), &summary),
                    after.as_ref().map_or_else(|| "absent".into(), summary)
                )
            }
            Delta::Via { id, before, after } => {
                let summary = |v: &viamind_doc::Via| {
                    format!(
                        "({}, {})Nm diameter={}Nm drill={}Nm net={}",
                        v.position.x.0,
                        v.position.y.0,
                        v.diameter.0,
                        v.drill.0,
                        v.net.as_deref().map_or_else(|| "unassigned".into(), abbreviated)
                    )
                };
                format!(
                    "Via {id}\nOld: {}\nNew: {}",
                    before.as_ref().map_or_else(|| "absent".into(), &summary),
                    after.as_ref().map_or_else(|| "absent".into(), summary)
                )
            }
            _ => "Unsupported change".into(),
        })
        .collect()
}
fn ghosts(deltas: Vec<Delta>) -> Vec<(Item, Color32)> {
    let mut before = Board::default();
    let mut after = Board::default();
    for delta in deltas {
        match delta {
            Delta::Part { id, before: b, after: a } => {
                if let Some(p) = b {
                    before.parts.insert(id, p);
                }
                if let Some(p) = a {
                    after.parts.insert(id, p);
                }
            }
            Delta::Track { id, before: b, after: a } => {
                if let Some(p) = b {
                    before.tracks.insert(id, p);
                }
                if let Some(p) = a {
                    after.tracks.insert(id, p);
                }
            }
            Delta::Via { id, before: b, after: a } => {
                if let Some(p) = b {
                    before.vias.insert(id, p);
                }
                if let Some(p) = a {
                    after.vias.insert(id, p);
                }
            }
            _ => {}
        }
    }
    let mut result = Vec::new();
    for (board, color) in [(before, REMOVED), (after, ADDED)] {
        let mut project = Project::new("branch ghost geometry");
        project.board = board;
        // Reuse production part/copper transforms and layer IDs. Boundary and
        // guide overlays belong to the live board and are deliberately omitted.
        result.extend(
            crate::board_scene::scene_from_project(&project)
                .items
                .into_iter()
                .filter(|i| i.layer != crate::layers::LayerId::EDGE_CUTS)
                .map(|item| (item, color)),
        );
    }
    result
}
impl ViamindApp {
    pub fn begin_pcb_branches(&mut self) {
        if self.design.is_none() || self.schematic.active || self.branch_incompatible() {
            return;
        }
        self.pcb_branches.open = true;
        self.pcb_branches.refresh(&self.design.as_ref().expect("design").engine);
    }
    fn branch_incompatible(&self) -> bool {
        self.files.busy()
            || self.parts.placing()
            || self.outlines.editing()
            || self.pads.editing()
            || self.copper.editing()
            || self.routing.editing()
            || self.track_cleanup.editing()
            || self.net_rules.editing()
    }
    pub(crate) fn sync_pcb_branches(&mut self, ctx: &egui::Context) {
        if self.schematic.active || self.branch_incompatible() || self.design.is_none() {
            self.pcb_branches.clear();
            return;
        }
        if self.pcb_branches.open || self.pcb_branches.shown {
            self.pcb_branches.refresh(&self.design.as_ref().expect("design").engine);
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
    pub(crate) fn paint_pcb_branch(&self, ui: &Ui, viewport: egui::Rect) {
        if !self.pcb_branches.visible() {
            return;
        }
        let Some(cache) = &self.pcb_branches.cached else { return };
        let painter = ui.painter_at(viewport);
        for (item, color) in &cache.items {
            if self.layers.get(item.layer).is_some_and(|l| l.visible) {
                painter.add(crate::canvas::item_shape(item, *color, &self.canvas.camera, viewport));
            }
        }
    }
    pub(crate) fn pcb_branches_window(&mut self, ui: &mut Ui) {
        self.pcb_branches.controls.clear();
        if !self.pcb_branches.open || self.branch_incompatible() || self.schematic.active {
            return;
        }
        let Some(design) = &self.design else { return };
        let engine = design.engine.clone();
        let mut open = true;
        let mut inspect = None;
        let mut action = None;
        let mut refresh = false;
        egui::Window::new("Agent branches").id(egui::Id::new("pcb-branches-review")).open(&mut open).default_width(360.0).max_width(440.0).max_height((ui.ctx().content_rect().height()-80.0).max(120.0)).show(ui.ctx(),|ui| {
            egui::ScrollArea::vertical().show(ui,|ui| {
                ui.label("Private PCB proposals. DRC is unchecked.");
                ui.colored_label(ADDED,"Green: added / new geometry");
                ui.colored_label(REMOVED,"Red: removed / old geometry");
                ui.label("Merge requires the original unchanged document and selection cursor. Refresh does not rebase.");
                if ui.button("Refresh branches").clicked() { refresh=true; }
                if self.pcb_branches.branches.is_empty() { ui.label("No branches. An agent can create a PCB branch through MCP."); }
                for b in &self.pcb_branches.branches {
                    ui.separator();
                    ui.label(format!("{} · revision {} · {} changes",b.label,b.revision,b.changed_objects));
                    ui.label(format!("Creator: {:?}",b.creator));
                    ui.label(format!("Source: {} / {}",b.project_id,b.source_cursor.sequence));
                    ui.label(format!("{} · {} · {} seconds left",if b.stale {"Stale"} else {"Current"},if b.busy {"Busy"} else {"Ready"},b.remaining_ms/1000));
                    ui.horizontal_wrapped(|ui| {
                        let response=ui.button("Inspect diff");
                        self.pcb_branches.controls.push((response.rect,ui.clip_rect()));
                        if response.clicked() { inspect=Some(b.branch_id); }
                        let response=ui.add_enabled(!b.stale && !b.busy && b.changed_objects>0,egui::Button::new("Merge branch"));
                        self.pcb_branches.controls.push((response.rect,ui.clip_rect()));
                        if response.clicked() {action=Some((UiAction::MergePcbBranch,json!({"branch_id":b.branch_id,"expected_revision":b.revision,"expected_project":b.project_id,"expected_cursor":b.source_cursor})));}
                        let response=ui.add_enabled(!b.busy,egui::Button::new("Discard branch"));
                        self.pcb_branches.controls.push((response.rect,ui.clip_rect()));
                        if response.clicked() {action=Some((UiAction::DiscardPcbBranch,json!({"branch_id":b.branch_id,"expected_revision":b.revision})));}
                    });
                }
                if self.pcb_branches.selected.is_some() { ui.checkbox(&mut self.pcb_branches.shown,"Show branch diff"); }
                if let Some(cache)=&self.pcb_branches.cached {
                    ui.label("Changed objects (long text abbreviated; MCP diff retains full bounded values):");
                    for detail in &cache.details {ui.separator();ui.label(detail);}
                }
                if let Some(message)=&self.pcb_branches.message { ui.colored_label(Color32::LIGHT_RED,message); }
            });
        });
        self.pcb_branches.open = open;
        if let Some(id) = inspect {
            self.pcb_branches.selected = Some(id);
            self.pcb_branches.shown = true;
            refresh = true;
        }
        if let Some((action, input)) = action {
            match action.run(&engine, input) {
                Ok(_) => {
                    self.pcb_branches.clear();
                    self.pcb_branches.message = Some(
                        if action == UiAction::MergePcbBranch {
                            "Branch merged as one undoable change."
                        } else {
                            "Branch discarded; live document unchanged."
                        }
                        .into(),
                    );
                }
                Err(error) => self.pcb_branches.message = Some(error.to_string()),
            }
            refresh = true;
        }
        if refresh {
            self.pcb_branches.refresh(&engine);
        }
    }
}
