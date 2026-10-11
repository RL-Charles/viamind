//! Retained checked equivalent-track cleanup; local overlays never mutate the document.
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};

use egui::{Color32, Stroke, Ui};
use viamind_doc::{CopperLayer, ObjectId};
use viamind_engine::{CheckedCardinalCleanup, CleanupCardinalTracks, CleanupTracks, EngineHandle};

use crate::{ViamindApp, actions::UiAction, camera::WorldPos, layers::LayerId};

struct Draft {
    token: ObjectId,
    by_net: bool,
    net: String,
    layer: Option<CopperLayer>,
    rules: viamind_drc::DrcInput,
    request: Option<CleanupCardinalTracks>,
    checked: Option<CheckedCardinalCleanup>,
    message: Option<String>,
}
struct Pending {
    token: ObjectId,
    input: CleanupCardinalTracks,
    receiver: Receiver<Result<CheckedCardinalCleanup, String>>,
}
#[derive(Default)]
pub struct TrackCleanupUi {
    draft: Option<Draft>,
    pending: Option<Pending>,
}
impl TrackCleanupUi {
    pub fn editing(&self) -> bool {
        self.draft.is_some()
    }
    pub fn checked(&self) -> Option<&CheckedCardinalCleanup> {
        self.draft.as_ref().and_then(|d| d.checked.as_ref())
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn message(&self) -> Option<&str> {
        self.draft.as_ref().and_then(|d| d.message.as_deref())
    }
    pub(crate) fn cancel(&mut self) {
        self.draft = None;
    }
    fn poll(&mut self, ctx: &egui::Context, engine: &EngineHandle) {
        let current = engine.event_cursor();
        if let Some(draft) = &mut self.draft
            && draft.request.as_ref().is_some_and(|r| r.expected_cursor != current)
        {
            draft.request = None;
            draft.checked = None;
            draft.message = Some("Document or selection changed; preview again.".into());
        }
        let Some(pending) = &self.pending else { return };
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
                return;
            }
            Err(TryRecvError::Disconnected) => Err("Cleanup worker stopped; preview again.".into()),
        };
        let pending = self.pending.take().expect("completed cleanup worker");
        if let Some(draft) = &mut self.draft
            && draft.token == pending.token
            && draft.request.as_ref() == Some(&pending.input)
            && current == pending.input.expected_cursor
        {
            match result {
                Ok(plan) => draft.checked = Some(plan),
                Err(error) => draft.message = Some(error),
            }
        }
    }
    fn start(&mut self, engine: &EngineHandle, ctx: &egui::Context, input: CleanupCardinalTracks) {
        if self.pending.is_some() {
            return;
        }
        let Some(draft) = &mut self.draft else { return };
        draft.checked = None;
        draft.message = None;
        draft.request = Some(input.clone());
        let (sender, receiver) = sync_channel(1);
        let token = draft.token;
        let worker_input = input.clone();
        let engine = engine.clone();
        let ctx = ctx.clone();
        match std::thread::Builder::new().name("viamind-track-cleanup".into()).spawn(move || {
            let result = engine.plan_cardinal_track_cleanup(&worker_input).map_err(|e| e.to_string());
            let _ = sender.send(result);
            ctx.request_repaint();
        }) {
            Ok(_) => self.pending = Some(Pending { token, input, receiver }),
            Err(error) => draft.message = Some(error.to_string()),
        }
    }
}
impl ViamindApp {
    pub fn begin_track_cleanup(&mut self) {
        if self.schematic.active
            || self.files.busy()
            || self.parts.placing()
            || self.outlines.editing()
            || self.pads.editing()
            || self.copper.editing()
            || self.routing.editing()
            || self.schematic_wires.active()
            || self.schematic_object_draft_active()
            || self.track_cleanup.editing()
            || self.net_rules.editing()
        {
            return;
        }
        if self.design.is_none() {
            return;
        }
        self.selection.cancel_gestures();
        self.track_cleanup.draft = Some(Draft {
            token: ObjectId::new(),
            by_net: false,
            net: "N".into(),
            layer: None,
            rules: Default::default(),
            request: None,
            checked: None,
            message: None,
        });
    }
    pub(crate) fn sync_track_cleanup(&mut self, ctx: &egui::Context) {
        if self.schematic.active || self.files.busy() {
            self.track_cleanup.cancel();
        }
        if let Some(design) = &self.design {
            self.track_cleanup.poll(ctx, &design.engine);
        }
    }
    pub(crate) fn paint_track_cleanup(&self, ui: &Ui, viewport: egui::Rect) {
        let Some(plan) = self.track_cleanup.checked() else { return };
        let painter = ui.painter_at(viewport);
        for replacement in &plan.replacements {
            let layer = if replacement.after.layer == CopperLayer::Bottom { LayerId::B_CU } else { LayerId::F_CU };
            if self.layers.get(layer).is_none_or(|l| !l.visible) {
                continue;
            }
            let screen = |p: viamind_geom::Point| self.canvas.camera.world_to_screen(WorldPos::new(p.x.to_mm(), p.y.to_mm()), viewport);
            painter.line_segment([screen(replacement.after.start), screen(replacement.after.end)], Stroke::new(3.0, Color32::LIGHT_GREEN));
            for track in &replacement.before {
                for p in [track.start, track.end] {
                    if p != replacement.after.start && p != replacement.after.end {
                        painter.circle_stroke(screen(p), 6.0, Stroke::new(2.0, Color32::YELLOW));
                    }
                }
            }
        }
    }
    pub(crate) fn track_cleanup_window(&mut self, ui: &mut Ui) {
        let Some(design) = &self.design else { return };
        let engine = design.engine.clone();
        let Some(draft) = &mut self.track_cleanup.draft else { return };
        let (project, cursor, ids, non_tracks) = engine.with_engine(|e| {
            let selection = e.selection();
            let ids: Vec<_> = selection.ids.iter().copied().filter(|id| e.project().board.tracks.contains_key(id)).take(2049).collect();
            let non_tracks = selection.ids.len().saturating_sub(selection.ids.iter().filter(|id| e.project().board.tracks.contains_key(id)).count());
            (e.project().id, e.event_cursor(), ids, non_tracks)
        });
        let mut open = true;
        let mut preview = false;
        let mut apply = false;
        let mut cancel = false;
        let pending = self.track_cleanup.pending.is_some();
        let mut changed = false;
        let content = ui.ctx().content_rect();
        egui::Window::new("Clean cardinal track joints").open(&mut open).default_width(440.0)
            .max_width((content.width()-24.0).max(240.0)).max_height((content.height()-24.0).max(160.0)).resizable(true).show(ui.ctx(), |ui| {
            egui::ScrollArea::vertical().max_height((content.height()-120.0).clamp(80.0,650.0)).show(ui, |ui| {
                ui.label("Equivalent horizontal/vertical straight joints only. Green: proposed survivor; yellow rings: removed joints.");
                ui.label("Select tracks on the board before opening, or choose an exact net below. Branches, pads, vias and drill-hole contacts stay in place.");
                ui.horizontal(|ui| {
                    changed |= ui.radio_value(&mut draft.by_net, false, "Selected tracks").changed();
                    changed |= ui.radio_value(&mut draft.by_net, true, "Named net").changed();
                });
                if draft.by_net {
                    ui.horizontal(|ui| { let label=ui.label("Exact net"); changed |= ui.text_edit_singleline(&mut draft.net).labelled_by(label.id).changed(); });
                    ui.horizontal(|ui| {
                        changed |= ui.radio_value(&mut draft.layer, None, "Both sides").changed();
                        changed |= ui.radio_value(&mut draft.layer, Some(CopperLayer::Top), "Top only").changed();
                        changed |= ui.radio_value(&mut draft.layer, Some(CopperLayer::Bottom), "Bottom only").changed();
                    });
                } else {
                    ui.label(format!("{} selected tracks; {} selected parts/vias excluded", ids.len(), non_tracks));
                }
                ui.collapsing("Scoped DRC rules (mm)", |ui| {
                    for (label, value) in [
                        ("Copper clearance", &mut draft.rules.copper_clearance_mm),
                        ("Board edge clearance", &mut draft.rules.edge_clearance_mm),
                        ("Minimum via drill", &mut draft.rules.minimum_via_drill_mm),
                        ("Minimum annular width", &mut draft.rules.minimum_annular_width_mm),
                        ("Hole spacing", &mut draft.rules.hole_clearance_mm),
                        ("Hole to copper", &mut draft.rules.hole_to_copper_clearance_mm),
                    ] { ui.horizontal(|ui| { ui.label(label); changed |= ui.add(egui::DragValue::new(value).range(0.0..=100.0).speed(0.01)).changed(); }); }
                });
                if changed { draft.checked = None; draft.request = None; draft.message = None; }
                if pending { ui.label("Checking captured board…"); }
                if let Some(plan) = &draft.checked {
                    crate::net_rules::show_document_minima(ui,&plan.document_net_rules);
                    let c = &plan.counts;
                    ui.label(format!("Checked: {} tracks to {} chains, {} redundant segments removed", c.selected_tracks, c.merged_chains, c.deleted_tracks));
                    ui.label(format!("Retained: {} anchors, {} branches, {} mismatches, {} hole chains, {} duplicates; {} diagonals, {} unassigned", c.retained_anchor_joints, c.retained_branch_joints, c.retained_mismatch_joints, c.retained_hole_chains, c.retained_duplicate_chains, c.unsupported_diagonal_tracks, c.unsupported_unassigned_tracks));
                    ui.label("Complete scoped joint DRC; existing unrelated violations and checker exclusions may remain. Apply recomputes with the full cursor guard.");
                    ui.label(format!("Actual coverage: {} pads, {} tracks, {} vias; {} excluded parts; {} existing unrelated violations", plan.coverage.checked_pads, plan.coverage.checked_tracks, plan.coverage.checked_vias, plan.coverage.excluded_part_count, plan.existing_unrelated_violations));
                    ui.collapsing("Checks and excluded parts", |ui| {
                        for check in &plan.coverage.checks_not_performed { ui.label(check); }
                        for part in &plan.coverage.excluded_parts { ui.label(format!("{}: {}",part.designator,part.reason)); }
                        if plan.coverage.excluded_details_truncated { ui.label("Further excluded-part details omitted by the checker; the total above includes them."); }
                    });
                }
                if let Some(message) = &draft.message { ui.colored_label(Color32::LIGHT_RED, message); }
            });
                ui.horizontal_wrapped(|ui| {
                    preview = ui.add_enabled(!pending && (draft.by_net || (!ids.is_empty() && ids.len() <= 2048)), egui::Button::new("Preview cleanup")).clicked();
                    apply = ui.add_enabled(!pending && draft.checked.as_ref().is_some_and(|p| p.counts.deleted_tracks > 0) && draft.request.as_ref().is_some_and(|r| r.expected_cursor == cursor), egui::Button::new("Apply cleanup")).clicked();
                    cancel = ui.button("Cancel cleanup").clicked();
                });
        });
        cancel |= !open || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel {
            self.track_cleanup.cancel();
            return;
        }
        if preview {
            let input = CleanupCardinalTracks {
                expected_project: project,
                expected_cursor: cursor,
                tracks: if draft.by_net { CleanupTracks::Net { net: draft.net.clone(), layer: draft.layer } } else { CleanupTracks::Ids { ids } },
                rules: draft.rules.clone(),
            };
            self.track_cleanup.start(&engine, ui.ctx(), input);
        } else if apply {
            let input = draft.request.clone().expect("checked request");
            match UiAction::CleanupCardinalTracks.run(&engine, serde_json::to_value(input).expect("typed cleanup serializes")) {
                Ok(_) => self.track_cleanup.cancel(),
                Err(error) => {
                    draft.checked = None;
                    draft.request = None;
                    draft.message = Some(error.to_string());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_retained_worker_cannot_publish_into_reopened_draft() {
        let engine = EngineHandle::new(viamind_doc::Project::new("retained cleanup"));
        let input = CleanupCardinalTracks {
            expected_project: engine.read(|p| p.id),
            expected_cursor: engine.event_cursor(),
            tracks: CleanupTracks::Net { net: "N".into(), layer: None },
            rules: Default::default(),
        };
        let token = ObjectId::new();
        let (sender, receiver) = sync_channel(1);
        let mut ui = TrackCleanupUi { draft: None, pending: Some(Pending { token, input: input.clone(), receiver }) };
        ui.cancel();
        assert!(ui.busy());
        ui.draft = Some(Draft {
            token: ObjectId::new(),
            by_net: true,
            net: "N".into(),
            layer: None,
            rules: Default::default(),
            request: Some(input.clone()),
            checked: None,
            message: None,
        });
        sender
            .send(Ok(CheckedCardinalCleanup {
                document_net_rules: Default::default(),
                project_id: input.expected_project,
                source_cursor: input.expected_cursor,
                counts: Default::default(),
                replacements: vec![],
                coverage: engine.read(|p| viamind_drc::run_drc(p, &Default::default()).unwrap().coverage),
                existing_unrelated_violations: 0,
                complete_scoped_preflight: true,
            }))
            .unwrap();
        ui.poll(&egui::Context::default(), &engine);
        assert!(!ui.busy());
        assert!(ui.checked().is_none());
    }
}
