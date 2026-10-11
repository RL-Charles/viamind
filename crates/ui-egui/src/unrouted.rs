//! Retained whole-board connectivity observation; no local editing tool state.
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::thread::JoinHandle;

use crate::ViamindApp;
use egui::Ui;
use viamind_engine::{EngineHandle, UnroutedNetQuery, UnroutedNetResult, UnroutedNetStatus};

struct Pending {
    receiver: Receiver<Result<UnroutedNetResult, String>>,
    _worker: JoinHandle<()>,
}
#[derive(Default)]
pub struct UnroutedUi {
    open: bool,
    pending: Option<Pending>,
    report: Option<UnroutedNetResult>,
    message: Option<String>,
}
impl UnroutedUi {
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn report(&self) -> Option<&UnroutedNetResult> {
        self.report.as_ref()
    }
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
    pub fn close(&mut self) {
        self.open = false;
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending else { return };
        let received = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
                return;
            }
            Err(TryRecvError::Disconnected) => Err("Unrouted-net worker stopped; retry the observation.".into()),
        };
        self.pending = None;
        match received {
            Ok(report) => {
                self.report = Some(report);
                self.message = None;
            }
            Err(error) => self.message = Some(error),
        }
    }
    fn start(&mut self, engine: &EngineHandle, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        self.message = None;
        let work = match engine.prepare_unrouted_nets(UnroutedNetQuery {}) {
            Ok(work) => work,
            Err(error) => {
                self.message = Some(error.to_string());
                return;
            }
        };
        let (sender, receiver) = sync_channel(1);
        let repaint = ctx.clone();
        match std::thread::Builder::new().name("viamind-unrouted-nets".into()).spawn(move || {
            let result = work.run().map_err(|e| e.to_string());
            let _ = sender.send(result);
            repaint.request_repaint();
        }) {
            Ok(worker) => self.pending = Some(Pending { receiver, _worker: worker }),
            Err(error) => self.message = Some(format!("Could not start unrouted-net worker: {error}")),
        }
    }
}
impl ViamindApp {
    pub fn begin_unrouted_nets(&mut self) {
        if self.design.is_some() {
            self.unrouted.open = true;
        }
    }
    pub(crate) fn unrouted_window(&mut self, ui: &mut Ui) {
        // Poll even when hidden: one worker/result slot remains owned by this UI.
        self.unrouted.poll(ui.ctx());
        if !self.unrouted.open {
            return;
        }
        let mut open = self.unrouted.open;
        egui::Window::new("Unrouted nets").open(&mut open).default_width(440.0).vscroll(true).show(ui.ctx(), |ui| {
            ui.label("Exact cardinal outer-copper terminal connectivity; not physical continuity, DRC or manufacturing certification.");
            ui.label(
                "Zero or one terminal has no assessed interterminal routing obligation. Unknown coverage prevents a whole-board routed conclusion.",
            );
            if ui.add_enabled(!self.unrouted.busy(), egui::Button::new("Query all nets")).clicked()
                && let Some(design) = &self.design
            {
                self.unrouted.start(&design.engine, ui.ctx());
            }
            if self.unrouted.busy() {
                ui.label("Analyzing bounded copper snapshot…");
            }
            if let Some(message) = &self.unrouted.message {
                ui.label(message);
            }
            if let Some(report) = &self.unrouted.report {
                ui.label(format!("Project: {}", report.project_id));
                ui.label(format!("Source: {} / {}", report.source_cursor.stream_id, report.source_cursor.sequence));
                if self.design.as_ref().is_some_and(|d| d.engine.event_cursor() != report.source_cursor) {
                    ui.label("Document or selection changed since this result; query again.");
                }
                ui.label(format!("Coverage: {}", if report.complete { "complete for supported model" } else { "incomplete" }));
                ui.label(format!(
                    "{} nets · {} unrouted · {} unknown · {} connected · {} with fewer than two terminals",
                    report.nets.len(),
                    report.unrouted_count,
                    report.unknown_count,
                    report.connected_count,
                    report.insufficient_terminal_count
                ));
                ui.label(format!(
                    "{} legacy components excluded · {} unsupported objects · {} unassigned objects · {} candidate visits",
                    report.coverage.excluded_legacy_parts,
                    report.coverage.unsupported_objects,
                    report.coverage.unassigned_objects,
                    report.coverage.candidate_visits
                ));
                for exclusion in &report.exclusions {
                    ui.label(exclusion);
                }
                ui.separator();
                for row in &report.nets {
                    let status = match row.status {
                        UnroutedNetStatus::Connected => "connected",
                        UnroutedNetStatus::Disconnected => "unrouted",
                        UnroutedNetStatus::InsufficientTerminals => "fewer than two terminals",
                        UnroutedNetStatus::Unknown => "unknown",
                    };
                    ui.label(format!(
                        "{}: {} · {} terminals · islands {}",
                        row.name,
                        status,
                        row.terminals,
                        row.terminal_islands.map_or_else(|| "unknown".into(), |n| n.to_string())
                    ));
                    if !row.reasons.is_empty() {
                        ui.label(format!("Coverage reasons: {:?}", row.reasons));
                    }
                }
            }
        });
        self.unrouted.open = open;
    }
}
