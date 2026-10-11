//! Actual egui input prepares production UI; native WGPU Screenshot readback closes this host.
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use std::{io::Write, path::PathBuf};
use viamind_engine::{DocumentStatus, EngineHandle, SessionSnapshot};
use viamind_ui_egui::{ViamindApp, actions::UiAction};
struct Inspect {
    app: ViamindApp,
    engine: EngineHandle,
    before: SessionSnapshot,
    status: DocumentStatus,
    capture: Option<PathBuf>,
    ui_passes: usize,
    controls: Vec<egui::Rect>,
}
impl eframe::App for Inspect {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        if self.capture.is_none() {
            // Skip the production dirty-document close dialog once immutable proof is saved.
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        assert_eq!(self.engine.session_state(), self.before);
        assert_eq!(self.engine.document_status(), self.status);
        self.ui_passes += 1;
        let content = ui.ctx().content_rect();
        assert!(self.controls.iter().all(|r| content.contains_rect(*r)), "prepared controls must fit actual native viewport");
        let image = ui.ctx().input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            assert!(image.pixels.len() <= 4_194_304);
            assert!(image.pixels.iter().all(|p| p.a() == 255));
            let path = self.capture.take().expect("one new capture");
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).expect("NEW readback");
            write!(file, "P6\n{} {}\n255\n", image.size[0], image.size[1]).unwrap();
            let pixels: Vec<_> = image.pixels.iter().flat_map(|p| [p.r(), p.g(), p.b()]).collect();
            file.write_all(&pixels).unwrap();
            file.sync_all().unwrap();
            println!(
                "{}",
                serde_json::json!({"capture":"eframe native WGPU Screenshot event", "pid":std::process::id(), "path":path, "pixels":image.size, "ui_passes":self.ui_passes, "full_session_unchanged":true, "document_status_unchanged":true, "self_close_after_capture":true, "controls":self.controls.iter().map(|r| [r.min.x,r.min.y,r.max.x,r.max.y]).collect::<Vec<_>>(), "content_rect":[content.min.x,content.min.y,content.max.x,content.max.y]})
            );
            std::io::stdout().flush().unwrap();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            if self.ui_passes == 8 {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            assert!(self.ui_passes < 180, "native screenshot did not complete");
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
}
fn click(h: &mut Harness<'_, ViamindApp>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(6);
    h.get_by_label(label).click();
    h.run_steps(5);
}
fn input(h: &mut Harness<'_, ViamindApp>, label: &str, text: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_steps(6);
    h.get_by_label(label).focus();
    for pressed in [true, false] {
        h.event(egui::Event::Key { key: egui::Key::A, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::COMMAND });
        h.run_steps(2);
    }
    h.get_by_label(label).type_text(text);
    h.run_steps(3);
}
fn wait(h: &mut Harness<'_, ViamindApp>, label: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.query_by_label_contains(label).is_none() {
        assert!(std::time::Instant::now() < deadline, "missing {label}");
        h.run_steps(3);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(5);
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = std::env::args().nth(1).ok_or("native symbol source required")?;
    let output = std::env::args().nth(2).ok_or("NEW output required")?;
    let mode = std::env::args().nth(3).unwrap_or_else(|| "preview".into());
    let width = std::env::args().nth(4).map_or(Ok(1600.0), |v| v.parse::<f32>())?;
    let height = std::env::args().nth(5).map_or(Ok(1200.0), |v| v.parse::<f32>())?;
    let capture = std::env::args_os().nth(6).map(PathBuf::from).ok_or("NEW PPM capture required")?;
    let engine = EngineHandle::new(viamind_doc::file::from_str(include_str!("../../../fixtures/schematic/regulator.viamind.json"))?);
    UiAction::PlacePart
        .run(&engine, serde_json::json!({"designator":"KEEP1","value":"dirty work preserved","footprint":"Original","x_mm":0,"y_mm":0}))?;
    let before = engine.session_state();
    let status = engine.document_status();
    let mut app = ViamindApp::with_design(engine.clone(), None);
    app.begin_symbol_library();
    let mut h = Harness::builder().with_size(egui::vec2(width, height)).build_ui_state(|ui, a: &mut ViamindApp| a.show(ui, None), app);
    wait(&mut h, "Target: Power regulator");
    input(&mut h, "Symbol library file path", &source);
    click(&mut h, "Load symbol template");
    wait(&mut h, "Loaded Original regulator");
    click(&mut h, "New detached definition");
    click(&mut h, "Add definition pin");
    input(&mut h, "Definition name", "Original stable symbol fields");
    input(&mut h, "Pin 2 name", "Retained input");
    input(&mut h, "Pin 2 X (mm)", "1.000003");
    input(&mut h, "Pin 2 Y (mm)", "-2.000007");
    let fields = ["number", "name", "X (mm)", "Y (mm)", "electrical type"];
    let ids: Vec<_> = fields.iter().map(|field| h.get_by_label(&format!("Pin 2 {field}")).accesskit_node().id()).collect();
    // Virtualized overscan nodes can exist outside the inner clip. Return the
    // actual pin viewport to its top before targeting the first row's button.
    h.get_by_label("Pin 2 name").scroll_to_me();
    h.run_steps(6);
    h.get_by_label("Pin 2 name").focus();
    h.run_steps(5);
    h.hover_at(h.get_by_label("Pin 2 name").rect().center());
    h.run_steps(2);
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        phase: egui::TouchPhase::Move,
        delta: egui::vec2(0.0, 4000.0),
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(8);
    click(&mut h, "Remove pin 1");
    assert!(h.query_by_label("1 / 1024 ordered pins").is_some(), "actual Remove must reduce two pins to one");
    for (i, field) in fields.iter().enumerate() {
        assert_eq!(h.get_by_label(&format!("Pin 1 {field}")).accesskit_node().id(), ids[i]);
    }
    click(&mut h, "Pin 1 electrical type");
    click(&mut h, "Input");
    input(&mut h, "NEW definition file path", &output);
    if height >= 800.0 {
        let id = egui::Id::new(Some(std::borrow::Cow::Borrowed("Native symbol library")));
        let window = h.ctx.memory(|m| m.area_rect(id)).unwrap();
        let corner = window.max - egui::vec2(3.0, 3.0);
        let target = egui::pos2(corner.x, height - 60.0);
        h.hover_at(corner);
        h.run_steps(3);
        h.drag_at(corner);
        h.run_steps(2);
        h.hover_at(target);
        h.run_steps(6);
        h.drop_at(target);
        h.run_steps(6);
    }
    if mode == "completed" {
        click(&mut h, "Save NEW definition file");
        wait(&mut h, "Created detached definition");
        let saved = viamind_doc::symbol_library::load(std::path::Path::new(&output))?;
        assert_eq!(saved.pins[0].name, "Retained input");
        assert_eq!(saved.pins[0].position.x.0, 1_000_003);
        assert_eq!(saved.pins[0].position.y.0, -2_000_007);
        assert_eq!(saved.pins[0].electrical, viamind_doc::schematic::PinType::Input);
    }
    let label = if mode == "completed" { "Created detached definition" } else { "Save NEW definition file" };
    h.get_by_label_contains(label).scroll_to_me();
    h.run_steps(8);
    let controls = vec![h.get_by_label_contains(label).rect()];
    assert!(controls.iter().all(|r| r.min.x >= 0.0 && r.min.y >= 0.0 && r.max.x <= width && r.max.y <= height));
    assert_eq!(engine.session_state(), before);
    assert_eq!(engine.document_status(), status);
    println!(
        "{}",
        serde_json::json!({"actual_input_identity_after_removal":true,"field_ids":ids.iter().map(|id|format!("{id:?}")).collect::<Vec<_>>(),"full_session_unchanged":true,"document_status_unchanged":true})
    );
    let memory = h.ctx.memory(Clone::clone);
    let mut app = h.into_state();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind stable symbol field inspection")
            .with_app_id("viamind-stable-symbol-inspection")
            .with_inner_size([width, height])
            .with_min_inner_size([width, height])
            .with_max_inner_size([width, height])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Viamind stable symbol field inspection",
        options,
        Box::new(move |cc| {
            app.configure_renderer(cc);
            cc.egui_ctx.memory_mut(|m| *m = memory);
            Ok(Box::new(Inspect { app, engine, before, status, capture: Some(capture), ui_passes: 0, controls }))
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
