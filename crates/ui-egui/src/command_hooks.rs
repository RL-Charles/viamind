//! Shared trusted hook configuration and bounded diagnostic inspection.
use crate::{ViamindApp, actions::UiAction};
use viamind_engine::CommandHookConfig;

#[derive(Default)]
pub struct CommandHooksUi {
    open: bool,
    draft: CommandHookConfig,
    synced_generation: Option<u64>,
    message: Option<String>,
    controls: Vec<(egui::Rect, egui::Rect)>,
}
impl CommandHooksUi {
    pub fn control_rects(&self) -> &[(egui::Rect, egui::Rect)] {
        &self.controls
    }
}
impl ViamindApp {
    pub fn begin_command_hooks(&mut self) {
        if self.design.is_some() {
            self.command_hooks.open = true;
        }
    }
    pub(crate) fn command_hooks_window(&mut self, ui: &mut egui::Ui) {
        self.command_hooks.controls.clear();
        if !self.command_hooks.open {
            return;
        }
        let Some(design) = &self.design else {
            self.command_hooks.open = false;
            return;
        };
        let status = design.engine.command_hook_status();
        if self.command_hooks.synced_generation != Some(status.generation) {
            self.command_hooks.draft = status.config.clone();
            self.command_hooks.synced_generation = Some(status.generation);
        }
        let content = ui.ctx().content_rect();
        let mut open = true;
        let mut requested = None;
        egui::Window::new("Command hooks").open(&mut open).collapsible(false).resizable(false)
            .default_width(440.).max_width((content.width()-32.).max(180.)).show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    let apply = ui.button("Apply hook settings");
                    self.command_hooks.controls.push((apply.rect,ui.clip_rect()));
                    if apply.clicked() { requested = Some(self.command_hooks.draft.clone()); }
                    let disable = ui.button("Disable veto");
                    self.command_hooks.controls.push((disable.rect,ui.clip_rect()));
                    if disable.clicked() { let mut config=status.config.clone();config.enabled=false;requested=Some(config); }
                });
                ui.label(format!("Gate {} · configuration {}",if status.config.enabled {"enabled"}else{"disabled"},status.generation));
                if let Some(last) = status.diagnostics.last() { ui.label(format!("Last {:?} failure: {} (details below)",last.phase,last.callback_id)); }
                if let Some(message)=&self.command_hooks.message { ui.label(message); }
                egui::ScrollArea::vertical().id_salt("command-hook-settings").max_height((content.height()-220.).clamp(60.,420.)).show(ui,|ui| {
                    ui.checkbox(&mut self.command_hooks.draft.enabled,"Enable scoped DRC veto");
                    ui.horizontal(|ui| {ui.label("Maximum objects");ui.add(egui::DragValue::new(&mut self.command_hooks.draft.max_objects).range(1..=10000));});
                    let rules = &mut self.command_hooks.draft.rules;
                    for (name,value) in [
                        ("Copper clearance mm",&mut rules.copper_clearance_mm),
                        ("Board edge clearance mm",&mut rules.edge_clearance_mm),
                        ("Minimum via drill mm",&mut rules.minimum_via_drill_mm),
                        ("Minimum annular width mm",&mut rules.minimum_annular_width_mm),
                        ("Hole clearance mm",&mut rules.hole_clearance_mm),
                        ("Hole to copper clearance mm",&mut rules.hole_to_copper_clearance_mm),
                    ] { ui.horizontal(|ui| {ui.label(name);ui.add(egui::DragValue::new(value).range(0.0..=100.0).speed(0.01));}); }
                    ui.horizontal(|ui| {ui.label("Maximum violations");ui.add(egui::DragValue::new(&mut rules.max_violations).range(1..=10000));});
                    ui.label("Changed apply and redo candidates must pass scoped DRC. Undo always remains available. A batch can repair its final candidate atomically. Enabling does not validate the current design.");
                    ui.label(status.supported_scope);
                    ui.label("Pre callbacks in order:");for id in &status.pre_callbacks {ui.monospace(id);}
                    ui.label("Post callbacks in order:");for id in &status.post_callbacks {ui.monospace(id);}
                    for diagnostic in status.diagnostics.iter().rev() {
                        ui.separator();ui.label(format!("{} {:?} · {}:{}",diagnostic.callback_id,diagnostic.phase,diagnostic.source_cursor.stream_id,diagnostic.source_cursor.sequence));
                        ui.label(&diagnostic.message);if diagnostic.truncated {ui.label("Diagnostic abbreviated at1024UTF-8 bytes.");}
                    }
                    ui.label("Trusted native Rust callbacks are registered by the host. MCP cannot submit code. Callback execution has no hard deadline.");
                });
            });
        self.command_hooks.open = open;
        if let Some(config) = requested {
            self.command_hooks.message = Some(
                match UiAction::ConfigureCommandHooks.run(&design.engine, serde_json::to_value(config).expect("finite validated hook editor")) {
                    Ok(_) => "Hook settings applied.".into(),
                    Err(e) => e.to_string(),
                },
            );
        }
        if open {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}
