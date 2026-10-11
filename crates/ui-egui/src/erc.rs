//! Manual bounded background ERC, with document and policy freshness guards.
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};

use egui::Ui;
use viamind_doc::ObjectId;
use viamind_engine::{EngineHandle, ErcInput, ErcReport, ErcSeverity, EventCursor};

use crate::ViamindApp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Document {
    project: ObjectId,
    stream: ObjectId,
}
impl Document {
    fn current(engine: &EngineHandle) -> (Self, EventCursor) {
        engine.with_engine(|e| {
            let cursor = e.event_cursor();
            (Self { project: e.project().id, stream: cursor.stream_id }, cursor)
        })
    }
}
struct Pending {
    document: Document,
    receiver: Receiver<Result<ErcReport, String>>,
}
pub struct ErcUi {
    open: bool,
    document: Option<Document>,
    input: ErcInput,
    pending: Option<Pending>,
    report: Option<ErcReport>,
    message: Option<String>,
}
fn default_input() -> ErcInput {
    let mut input = ErcInput::default();
    input.compatibility = input.effective_matrix().expect("valid native default policy");
    input
}
impl Default for ErcUi {
    fn default() -> Self {
        Self { open: false, document: None, input: default_input(), pending: None, report: None, message: None }
    }
}
impl ErcUi {
    fn sync(&mut self, document: Option<Document>) {
        if self.document != document {
            self.document = document;
            self.report = None;
            self.message = None;
            // Retain the receiver until completion: even replacement cannot spawn a second job.
        }
        let Some(pending) = &self.pending else { return };
        let result = match pending.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("ERC worker stopped; run ERC again".into())),
        };
        if let Some(result) = result {
            let pending = self.pending.take().expect("pending result");
            if Some(pending.document) != self.document {
                return;
            }
            match result {
                Ok(report) if report.project_id == pending.document.project && report.source_cursor.stream_id == pending.document.stream => {
                    self.report = Some(report);
                    self.message = None;
                }
                Ok(_) => self.message = Some("Document changed during ERC; run ERC again".into()),
                Err(error) => self.message = Some(error),
            }
        }
    }
    fn start(&mut self, engine: EngineHandle) {
        if self.pending.is_some() {
            return;
        }
        let (document, _) = Document::current(&engine);
        self.sync(Some(document));
        let input = self.input.clone();
        if let Err(error) = input.effective_matrix() {
            self.message = Some(error.to_string());
            return;
        }
        let (sender, receiver) = sync_channel(1);
        match std::thread::Builder::new().name("viamind-erc".into()).spawn(move || {
            let result = engine.run_erc(&input).map_err(|e| e.to_string());
            let _ = sender.try_send(result);
        }) {
            Ok(_) => {
                self.pending = Some(Pending { document, receiver });
                self.message = None;
            }
            Err(error) => self.message = Some(format!("Could not start ERC: {error}")),
        }
    }
    fn fresh(&self, cursor: EventCursor) -> bool {
        self.report.as_ref().is_some_and(|report| {
            self.document == Some(Document { project: report.project_id, stream: report.source_cursor.stream_id })
                && report.source_cursor == cursor
                && report.policy == self.input
        })
    }
}
fn severity(ui: &mut Ui, name: &str, value: &mut ErcSeverity) {
    ui.horizontal(|ui| {
        let caption = ui.label(name);
        egui::ComboBox::from_id_salt(name)
            .selected_text(format!("{value:?}"))
            .show_ui(ui, |ui| {
                for option in [ErcSeverity::Allow, ErcSeverity::Warning, ErcSeverity::Error] {
                    ui.selectable_value(value, option, format!("{option:?}"));
                }
            })
            .response
            .labelled_by(caption.id);
    });
}
impl ViamindApp {
    pub fn begin_erc(&mut self) {
        if self.design.is_some() {
            self.erc.open = true;
        }
    }
    /// Runs the same bounded native check exposed as `run_erc` over MCP.
    pub fn run_design_erc(&mut self) {
        if let Some(design) = &self.design {
            self.erc.start(design.engine.clone());
        }
    }
    pub(crate) fn erc_window(&mut self, ui: &mut Ui) {
        let current = self.design.as_ref().map(|d| Document::current(&d.engine));
        self.erc.sync(current.map(|(document, _)| document));
        if self.erc.pending.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(20));
        }
        let Some((_, cursor)) = current else { return };
        if !self.erc.open {
            return;
        }
        let mut run = false;
        let mut navigate = None;
        let mut open = self.erc.open;
        egui::Window::new("Electrical rule check").open(&mut open).default_width(720.0).default_height(650.0).show(ui.ctx(), |ui| {
            ui.label("Native pin checks only; full ERC is not performed.");
            ui.collapsing("ERC rules", |ui| {
                for (name, value) in [
                    ("Unconnected input", &mut self.erc.input.unconnected_input),
                    ("Undriven input", &mut self.erc.input.undriven_input),
                    ("Missing power", &mut self.erc.input.missing_power),
                    ("No-connect attachment", &mut self.erc.input.no_connect_attached),
                ] {
                    severity(ui, name, value);
                }
                ui.horizontal(|ui| {
                    let label = ui.label("Maximum ERC issues");
                    ui.add(egui::DragValue::new(&mut self.erc.input.max_violations).range(1..=10000)).labelled_by(label.id);
                    if ui.button("Reset native ERC rules").clicked() {
                        self.erc.input = default_input();
                    }
                });
                ui.collapsing("Pin compatibility (66 unordered pairs)", |ui| {
                    egui::ScrollArea::vertical().id_salt("erc_matrix").max_height(200.0).show(ui, |ui| {
                        for rule in &mut self.erc.input.compatibility {
                            severity(ui, &format!("{:?} / {:?}", rule.a, rule.b), &mut rule.severity);
                        }
                    });
                });
            });
            run = ui.add_enabled(self.erc.pending.is_none(), egui::Button::new("Run ERC")).clicked();
            if self.erc.pending.is_some() {
                ui.label("ERC running; editing remains available.");
            }
            if let Some(message) = &self.erc.message {
                ui.label(message);
            }
            let fresh = self.erc.fresh(cursor);
            if let Some(report) = &self.erc.report {
                if report.source_cursor != cursor {
                    ui.label("ERC result is stale: document changed; run ERC again.");
                }
                if report.policy != self.erc.input {
                    ui.label("ERC result uses previous rules; run ERC again.");
                }
                if !report.complete {
                    ui.label(format!("Incomplete ERC: {}. No all-clear conclusion.", report.stop_reason.unwrap_or("work limit")));
                } else if report.violations.is_empty() {
                    ui.label(if fresh {
                        "No issues in supported checks. Full ERC is not performed."
                    } else {
                        "Previous snapshot: no issues in supported checks; run ERC for current results."
                    });
                }
                let errors = report.violations.iter().filter(|v| v.severity == ErcSeverity::Error).count();
                ui.label(format!(
                    "{} errors · {} warnings · {} pin pairs checked",
                    errors,
                    report.violations.len() - errors,
                    report.coverage.tested_pin_pairs
                ));
                ui.collapsing("ERC coverage and effective rules", |ui| {
                    ui.label(format!(
                        "{} nets visited · {} pins evaluated · {} unspecified pins",
                        report.coverage.visited_nets, report.coverage.evaluated_pins, report.coverage.unspecified_pins
                    ));
                    for exclusion in &report.coverage.checks_not_performed {
                        ui.label(format!("Excluded: {exclusion}"));
                    }
                    ui.label(format!("Source {}:{} · project {}", report.source_cursor.stream_id, report.source_cursor.sequence, report.project_id));
                    egui::ScrollArea::vertical().id_salt("erc_effective_policy").max_height(160.0).show(ui, |ui| {
                        ui.label(format!(
                            "Unconnected {:?} · undriven {:?} · missing power {:?} · no-connect {:?}",
                            report.policy.unconnected_input,
                            report.policy.undriven_input,
                            report.policy.missing_power,
                            report.policy.no_connect_attached
                        ));
                        for rule in &report.effective_matrix {
                            ui.monospace(format!("{:?} / {:?}: {:?}", rule.a, rule.b, rule.severity));
                        }
                    });
                });
                egui::ScrollArea::vertical().id_salt("erc_results").max_height(340.0).show_rows(ui, 180.0, report.violations.len(), |ui, rows| {
                    for row in rows {
                        let issue = &report.violations[row];
                        ui.push_id(&issue.id, |ui| {
                            let summary = format!("{:?}: {}", issue.severity, issue.message);
                            ui.add(egui::Label::new(&summary).truncate()).on_hover_text(&summary);
                            let net = format!("Net {} · {}", issue.net_name.as_deref().unwrap_or("unnamed"), issue.net_id);
                            ui.add(egui::Label::new(&net).truncate()).on_hover_text(&net);
                            ui.horizontal_wrapped(|ui| {
                                for (n, pin) in issue.pins.iter().enumerate() {
                                    if ui
                                        .add_enabled(
                                            fresh,
                                            egui::Button::new(format!(
                                                "Show pin {} · {}",
                                                pin.pin_number.as_deref().unwrap_or("?"),
                                                &pin.id.to_string()[28..]
                                            )),
                                        )
                                        .on_hover_text(format!(
                                            "Pin {}\nSheet {}\nSymbol {}",
                                            pin.id,
                                            pin.sheet_id,
                                            pin.symbol_id.map_or_else(|| "absent".into(), |id| id.to_string())
                                        ))
                                        .clicked()
                                    {
                                        navigate = Some((report.project_id, report.source_cursor, issue.pins[n].clone()));
                                    }
                                }
                            });
                            ui.separator();
                        });
                    }
                });
            }
        });
        self.erc.open = open;
        if run {
            self.run_design_erc();
        }
        if let Some((project, cursor, pin)) = navigate {
            self.erc.message = self.navigate_erc_pin(project, cursor, &pin).err();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::UiAction;
    use viamind_doc::Project;

    fn pending(ui: &mut ErcUi, engine: &EngineHandle) -> std::sync::mpsc::SyncSender<Result<ErcReport, String>> {
        let (document, _) = Document::current(engine);
        ui.sync(Some(document));
        let (sender, receiver) = sync_channel(1);
        ui.pending = Some(Pending { document, receiver });
        sender
    }
    #[test]
    fn one_pending_job_and_deterministic_policy_and_cursor_freshness() {
        let engine = EngineHandle::new(Project::new("Worker"));
        let mut ui = ErcUi::default();
        let sender = pending(&mut ui, &engine);
        ui.start(engine.clone());
        sender.send(Ok(engine.run_erc(&ui.input).unwrap())).unwrap();
        let (document, cursor) = Document::current(&engine);
        ui.sync(Some(document));
        assert!(ui.pending.is_none());
        assert!(ui.fresh(cursor));
        ui.input.missing_power = ErcSeverity::Allow;
        assert!(!ui.fresh(cursor));
        ui.input = default_input();
        assert!(ui.fresh(cursor));
        let changed = EventCursor { sequence: cursor.sequence + 1, ..cursor };
        assert!(!ui.fresh(changed));
    }
    #[test]
    fn replacement_and_same_id_reopen_reject_old_completions_and_keep_one_job() {
        let engine = EngineHandle::new(Project::new("Old"));
        let mut ui = ErcUi::default();
        let old = engine.run_erc(&ui.input).unwrap();
        let sender = pending(&mut ui, &engine);
        // Same persisted UUID in a different session must also invalidate old results.
        let reopened = EngineHandle::new(engine.read(Clone::clone));
        let (document, _) = Document::current(&reopened);
        ui.sync(Some(document));
        assert!(ui.pending.is_some());
        ui.start(reopened.clone());
        sender.send(Ok(old)).unwrap();
        ui.sync(Some(document));
        assert!(ui.pending.is_none());
        assert!(ui.report.is_none());
        let sender = pending(&mut ui, &reopened);
        let old = reopened.run_erc(&ui.input).unwrap();
        UiAction::NewProject.run(&reopened, serde_json::json!({"name":"Replacement"})).unwrap();
        let (document, _) = Document::current(&reopened);
        ui.sync(Some(document));
        sender.send(Ok(old)).unwrap();
        ui.sync(Some(document));
        assert!(ui.report.is_none());
    }
    #[test]
    fn completion_after_edits_remains_visible_but_stale_and_disconnect_is_actionable() {
        let engine = EngineHandle::new(Project::new("Edit"));
        let mut ui = ErcUi::default();
        let old = engine.run_erc(&ui.input).unwrap();
        let sender = pending(&mut ui, &engine);
        sender.send(Ok(old.clone())).unwrap();
        ui.sync(ui.document);
        assert!(ui.report.is_some());
        assert!(!ui.fresh(EventCursor { sequence: old.source_cursor.sequence + 1, ..old.source_cursor }));
        let sender = pending(&mut ui, &engine);
        drop(sender);
        ui.sync(ui.document);
        assert!(ui.pending.is_none());
        assert!(ui.message.as_ref().unwrap().contains("worker stopped"));
    }
}
