//! Explicit bus authoring and immutable scalar facts through the shared action catalog.
use crate::{ViamindApp, actions::UiAction};
use egui::{Context, Ui};
use serde_json::{Value, json};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use viamind_doc::{ObjectId, schematic::Coordinate};
use viamind_engine::{BusSource, EngineHandle, SchematicBusesResult, SetSchematicBus, UnfoldSchematicBusMember};
use viamind_geom::Nm;
struct Pending {
    receiver: Receiver<Result<Value, String>>,
    action: UiAction,
    abandoned: bool,
}
pub struct SchematicBusesUi {
    pub open: bool,
    source: Option<BusSource>,
    result: Option<SchematicBusesResult>,
    selected: Option<ObjectId>,
    selected_entry: Option<ObjectId>,
    pub expression: String,
    pub coordinates: [String; 10],
    pub member: String,
    preview: Option<UnfoldSchematicBusMember>,
    pending: Option<Pending>,
    pub message: Option<String>,
}
impl Default for SchematicBusesUi {
    fn default() -> Self {
        Self {
            open: false,
            source: None,
            result: None,
            selected: None,
            selected_entry: None,
            expression: "DATA[0..7]".into(),
            coordinates: ["0", "0", "20", "0", "0", "0", "2", "2", "2", "8"].map(str::to_owned),
            member: "DATA0".into(),
            preview: None,
            pending: None,
            message: None,
        }
    }
}
impl SchematicBusesUi {
    pub fn active(&self) -> bool {
        self.open || self.pending.is_some()
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    fn begin(&mut self, engine: &EngineHandle, sheet: ObjectId) {
        if self.busy() {
            return;
        }
        self.source = Some(engine.with_engine(|e| BusSource { expected_project: e.project().id, source_cursor: e.event_cursor(), sheet_id: sheet }));
        self.result = None;
        self.selected = None;
        self.selected_entry = None;
        self.preview = None;
        self.open = true;
        self.message =
            Some("Load scalar facts to inspect declarations. Coordinates are explicit millimetres; bus/entry graphics do not conduct.".into());
    }
    fn request(&self) -> Result<UnfoldSchematicBusMember, String> {
        let source = self.source.ok_or("Choose a schematic sheet")?;
        let bus_id = self.selected.ok_or("Choose a declared bus")?;
        let p = |a: usize| -> Result<Coordinate, String> {
            Ok(Coordinate { x: Nm(super::schematic::nm(&self.coordinates[a])?), y: Nm(super::schematic::nm(&self.coordinates[a + 1])?) })
        };
        Ok(UnfoldSchematicBusMember { source, bus_id, member: self.member.clone(), bus_point: p(4)?, wire_point: p(6)?, wire_end: p(8)? })
    }
    fn run(&mut self, engine: &EngineHandle, ctx: &Context, action: UiAction, input: Value) {
        if self.busy() {
            return;
        }
        let engine = engine.clone();
        let ctx = ctx.clone();
        let (tx, rx) = sync_channel(1);
        match std::thread::Builder::new().name("viamind-schematic-buses".into()).spawn(move || {
            let r = action.run(&engine, input).map_err(|e| e.to_string());
            let _ = tx.send(r);
            ctx.request_repaint();
        }) {
            Ok(_) => {
                self.pending = Some(Pending { receiver: rx, action, abandoned: false });
                self.message = Some(format!("Running {}…", action.tool()));
            }
            Err(e) => self.message = Some(format!("Could not start bus operation: {e}")),
        }
    }
    fn poll(&mut self, ctx: &Context) {
        let Some(p) = &self.pending else {
            return;
        };
        let r = match p.receiver.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(20));
                return;
            }
            Err(TryRecvError::Disconnected) => Err("Bus worker stopped before returning a result".into()),
        };
        let p = self.pending.take().expect("pending worker");
        if p.abandoned {
            return;
        }
        match r {
            Err(e) => {
                self.preview = None;
                self.message = Some(e);
            }
            Ok(v) if p.action == UiAction::QuerySchematicBuses => match serde_json::from_value::<SchematicBusesResult>(v) {
                Ok(r) => {
                    if let Some(source) = &mut self.source {
                        if source.expected_project == r.project_id {
                            source.source_cursor = r.source_cursor;
                        } else {
                            self.message = Some("Project changed; close and reopen the bus form".into());
                            return;
                        }
                    }
                    self.message = Some(format!(
                        "Complete: {} buses, {} entries, {} expanded members · {} scalar nets · source {}:{}",
                        r.total_buses,
                        r.total_entries,
                        r.total_expanded_members,
                        r.coverage.scalar_nets,
                        r.source_cursor.stream_id,
                        r.source_cursor.sequence
                    ));
                    self.result = Some(r);
                    self.preview = None;
                }
                Err(e) => self.message = Some(e.to_string()),
            },
            Ok(_) if p.action == UiAction::PreviewSchematicBusUnfold => match self.request() {
                Ok(request) => {
                    self.preview = Some(request);
                    self.message = Some(
                        "Preview passed actual scalar UUID partition and label-scope checks. Apply explicitly to commit one sheet transaction."
                            .into(),
                    );
                }
                Err(e) => self.message = Some(e),
            },
            Ok(_) => {
                self.preview = None;
                self.result = None;
                self.message =
                    Some("Committed one guarded sheet transaction. Load scalar facts to capture the new cursor before another edit.".into());
            }
        }
    }
    fn window(&mut self, ctx: &Context, engine: &EngineHandle) {
        if !self.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Explicit schematic buses").id(egui::Id::new("schematic_buses")).open(&mut open).vscroll(true).default_width(430.0).show(ctx,|ui|{
            ui.label("Bus graphics are nonconducting. Unfold creates an ordinary scalar wire and Local member label; no hierarchy or implicit global member connection.");
            let busy=self.busy();
            ui.add_enabled_ui(!busy,|ui|{
                if ui.button("Load scalar bus facts").clicked(){self.run(engine,ctx,UiAction::QuerySchematicBuses,json!({}));}
                let rows:Vec<_>=self.result.as_ref().map(|r|r.buses.iter().filter(|b|self.source.is_some_and(|s|s.sheet_id==b.sheet_id)).map(|b|(b.bus.id,b.bus.expression.clone(),b.bus.start,b.bus.end)).collect()).unwrap_or_default();
                egui::ComboBox::from_id_salt("bus_choice").selected_text(self.selected.map_or_else(||"New bus".into(),|id|id.to_string())).show_ui(ui,|ui|{
                    if ui.selectable_label(self.selected.is_none(),"New bus").clicked(){self.selected=None;self.selected_entry=None;self.preview=None;}
                    for(id,expression,a,z)in rows{if ui.selectable_label(self.selected==Some(id),&expression).clicked(){self.selected=Some(id);self.expression=expression;self.coordinates[0]=mm(a.x.0);self.coordinates[1]=mm(a.y.0);self.coordinates[2]=mm(z.x.0);self.coordinates[3]=mm(z.y.0);self.selected_entry=None;self.preview=None;}}
                });
                let label=ui.label("Bus expression");if ui.add(egui::TextEdit::singleline(&mut self.expression).char_limit(4096)).labelled_by(label.id).changed(){self.preview=None;}
                for(i,name)in["Bus start X mm","Bus start Y mm","Bus end X mm","Bus end Y mm"].iter().enumerate(){let label=ui.label(*name);if ui.add(egui::TextEdit::singleline(&mut self.coordinates[i]).char_limit(32)).labelled_by(label.id).changed(){self.preview=None;}}
                ui.horizontal_wrapped(|ui|{
                    if ui.button("Save bus declaration").clicked(){let r=(||->Result<Value,String>{let source=self.source.ok_or("Choose a sheet")?;let p=|i|->Result<Coordinate,String>{Ok(Coordinate{x:Nm(super::schematic::nm(&self.coordinates[i])?),y:Nm(super::schematic::nm(&self.coordinates[i+1])?)})};serde_json::to_value(SetSchematicBus{source,bus_id:self.selected,expression:self.expression.clone(),start:p(0)?,end:p(2)?}).map_err(|e|e.to_string())})();match r{Ok(v)=>self.run(engine,ctx,UiAction::SetSchematicBus,v),Err(e)=>self.message=Some(e)}}
                    if ui.add_enabled(self.selected.is_some(),egui::Button::new("Delete bus declaration")).clicked(){self.run(engine,ctx,UiAction::DeleteSchematicBus,json!({"source":self.source,"bus_id":self.selected}));}
                });
                ui.separator();
                let facts=self.result.as_ref().and_then(|r|r.buses.iter().find(|b|Some(b.bus.id)==self.selected));
                let members:Vec<_>=facts.map(|b|b.members.iter().map(|m|(m.name.clone(),m.labels.len(),m.missing)).collect()).unwrap_or_default();
                egui::ComboBox::from_id_salt("member_choice").selected_text(&self.member).show_ui(ui,|ui|{for(name,count,missing)in members{if ui.selectable_label(self.member==name,format!("{name} · {count} actual labels{}",if missing{" · missing"}else{""})).clicked(){self.member=name;self.preview=None;}}});
                for(i,name)in["Entry bus X mm","Entry bus Y mm","Entry wire X mm","Entry wire Y mm","Opposite wire end X mm","Opposite wire end Y mm"].iter().enumerate(){let label=ui.label(*name);if ui.add(egui::TextEdit::singleline(&mut self.coordinates[i+4]).char_limit(32)).labelled_by(label.id).changed(){self.preview=None;}}
                ui.horizontal_wrapped(|ui|{
                    if ui.add_enabled(self.selected.is_some(),egui::Button::new("Preview member unfolding")).clicked(){match self.request().and_then(|r|serde_json::to_value(r).map_err(|e|e.to_string())){Ok(v)=>self.run(engine,ctx,UiAction::PreviewSchematicBusUnfold,v),Err(e)=>self.message=Some(e)}}
                    if ui.add_enabled(self.preview.is_some(),egui::Button::new("Apply member unfolding")).clicked(){if let Some(r)=&self.preview{self.run(engine,ctx,UiAction::UnfoldSchematicBusMember,json!(r));}}
                });
                let entries:Vec<_>=self.result.as_ref().and_then(|r|r.buses.iter().find(|b|Some(b.bus.id)==self.selected)).map(|b|b.entries.iter().map(|e|(e.id,e.member.clone())).collect()).unwrap_or_default();
                egui::ComboBox::from_id_salt("entry_choice").selected_text(self.selected_entry.map_or_else(||"Choose owned breakout".into(),|id|id.to_string())).show_ui(ui,|ui|{for(id,member)in entries{if ui.selectable_label(self.selected_entry==Some(id),member).clicked(){self.selected_entry=Some(id);}}});
                if ui.add_enabled(self.selected_entry.is_some(),egui::Button::new("Delete complete owned breakout")).clicked(){self.run(engine,ctx,UiAction::DeleteSchematicBusBreakout,json!({"source":self.source,"entry_id":self.selected_entry}));}
            });
            if ui.button("Cancel bus form").clicked(){self.preview=None;if let Some(p)=&mut self.pending{p.abandoned=true;}self.open=false;}
            if let Some(m)=&self.message{ui.label(m);}
            if busy{ui.spinner();}
        });
        if !open {
            self.preview = None;
            if let Some(p) = &mut self.pending {
                p.abandoned = true;
            }
            self.open = false;
        }
    }
}
fn mm(n: i64) -> String {
    let x = i128::from(n);
    let abs = x.abs();
    format!("{}{}.{:06}", if x < 0 { "-" } else { "" }, abs / 1_000_000, abs % 1_000_000)
}
impl ViamindApp {
    pub(crate) fn sync_schematic_buses(&mut self, ctx: &Context) {
        self.schematic_buses.poll(ctx);
    }
    pub(crate) fn schematic_buses_toolbar(&mut self, ui: &mut Ui) {
        let sheet = self.schematic.active_sheet_id();
        let enabled =
            sheet.is_some() && !self.files.busy() && !self.schematic_wires.active() && !self.schematic_drag.active() && !self.schematic_buses.busy();
        if ui.add_enabled(enabled, egui::Button::new("Explicit buses and member unfolding")).clicked() {
            if let (Some(d), Some(sheet)) = (&self.design, sheet) {
                self.schematic_buses.begin(&d.engine, sheet);
                self.clear_schematic_object_draft();
            }
        }
    }
    pub(crate) fn schematic_buses_window(&mut self, ui: &Ui) {
        if let Some(d) = &self.design {
            self.schematic_buses.window(ui.ctx(), &d.engine);
        }
    }
}
