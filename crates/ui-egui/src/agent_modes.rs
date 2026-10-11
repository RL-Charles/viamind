//! Trusted human controls. No MCP tool changes admission policy.
use crate::ViamindApp;
use viamind_engine::agent_policy::AgentMode;
#[derive(Default)]
pub struct AgentModesUi {
    open: bool,
    message: Option<String>,
}
pub fn mode_label(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Live => "Agents live apply",
        AgentMode::Approval => "Agents approval",
        AgentMode::ReadOnly => "Agents read only",
    }
}
impl ViamindApp {
    pub fn begin_agent_modes(&mut self) {
        if self.design.is_some() {
            self.agent_modes.open = true;
        }
    }
    pub(crate) fn agent_modes_window(&mut self, ui: &mut egui::Ui) {
        if !self.agent_modes.open {
            return;
        }
        let Some(design) = &self.design else {
            self.agent_modes.open = false;
            return;
        };
        let policy = design.engine.agent_policy();
        let status = policy.status();
        let mut open = true;
        let mut requested = None;
        let content = ui.ctx().content_rect();
        egui::Window::new("Agent modes").open(&mut open).collapsible(false).resizable(false)
            .default_width(400.).max_width((content.width()-32.).max(180.)).show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                for (mode,label) in [(AgentMode::Live,"Live apply"),(AgentMode::Approval,"Approval"),(AgentMode::ReadOnly,"Read only")] {
                    if ui.add_enabled(!status.startup_locked || mode == AgentMode::ReadOnly, egui::Button::new(label).selected(status.mode == mode)).clicked() { requested = Some(mode); }
                }
            });
            egui::ScrollArea::vertical().max_height((content.height()-160.).clamp(60.,340.)).show(ui, |ui| {
                ui.strong(mode_label(status.mode));
                if status.startup_locked { ui.label("Startup read-only lock: restart without --agent-read-only to allow agent edits."); }
                ui.label("Live apply: agent writes apply immediately. Approval: agents prepare bounded private PCB proposals; a human accepts or rejects in Board → Agent branches. Unsupported edits and file publication refuse. Read only: observations and in-memory views.");
                ui.label("Human editing stays live. These controls change all agent endpoints in this application. They do not accept an existing proposal or change its lease.");
                ui.label(format!("Policy generation {} · unfinished agent operations {}",status.generation,status.active_writers));
                if let Some(message) = &self.agent_modes.message { ui.label(message); }
            });
        });
        self.agent_modes.open = open;
        if let Some(mode) = requested {
            self.agent_modes.message = Some(match policy.set_mode(mode) {
                Ok(status) => format!("{}.", mode_label(status.mode)),
                Err(error) => error.to_string(),
            });
        }
        if open {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}
