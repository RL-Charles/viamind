//! Native wgpu cadence benchmark; run in release with a new JSON output path.
use egui::{Ui, vec2};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use viamind_ui_egui::{ViamindApp, camera::Camera};

const WARMUP: usize = 60;
const SAMPLES: usize = 360;
const PHASES: [&str; 3] = ["fit_pan", "fit_pan_zoom", "zoomed_8x_pan"];
#[derive(Default)]
struct Phase {
    intervals: Vec<f64>,
    cpu: Vec<f64>,
    drawn: Vec<usize>,
    gpu_draw_calls: Vec<usize>,
    gpu_layers: Vec<usize>,
    tiles_built: Vec<usize>,
}
fn stats(values: &[f64]) -> Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    json!({"samples":sorted.len(),"median_ms":sorted[sorted.len()/2],"p95_ms":sorted[sorted.len()*95/100],"mean_ms":mean,"throughput_fps":1000.0/mean})
}
struct Benchmark {
    app: ViamindApp,
    phases: [Phase; 3],
    frame: usize,
    base: Option<Camera>,
    last_native_frame: Option<u64>,
    finished: bool,
    previous: Option<(Instant, usize, bool, usize, viamind_ui_egui::mesh_cache::CacheStats)>,
    adapter: Value,
    persistent_gpu: bool,
    output: std::fs::File,
    result: Arc<Mutex<Option<Result<(), String>>>>,
}
impl eframe::App for Benchmark {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        if self.finished {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let native_frame = ui.ctx().cumulative_frame_nr();
        // egui may run several layout passes within one native frame. Count it once.
        if self.last_native_frame == Some(native_frame) {
            self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
            ui.ctx().request_repaint();
            return;
        }
        self.last_native_frame = Some(native_frame);
        let now = Instant::now();
        if let Some((previous, phase, sample, drawn, cache)) = self.previous.take()
            && sample
        {
            self.phases[phase].intervals.push(now.duration_since(previous).as_secs_f64() * 1000.0);
            if let Some(cpu) = frame.info().cpu_usage {
                self.phases[phase].cpu.push(f64::from(cpu) * 1000.0);
            }
            self.phases[phase].drawn.push(drawn);
            self.phases[phase].gpu_draw_calls.push(cache.gpu_draw_calls);
            self.phases[phase].gpu_layers.push(cache.gpu_layers);
            self.phases[phase].tiles_built.push(cache.tiles_built);
        }
        let phase = self.frame / (WARMUP + SAMPLES);
        let local = self.frame % (WARMUP + SAMPLES);
        if phase == PHASES.len() {
            let phases:Vec<_>=self.phases.iter().enumerate().map(|(i,p)| {
                let intervals=stats(&p.intervals);
                json!({"name":PHASES[i],"frame_interval":intervals,"egui_cpu":if p.cpu.is_empty(){Value::Null}else{stats(&p.cpu)},"drawn_min":p.drawn.iter().min(),"drawn_max":p.drawn.iter().max(),"gpu_draw_calls_min":p.gpu_draw_calls.iter().min(),"gpu_draw_calls_max":p.gpu_draw_calls.iter().max(),"gpu_layers_max":p.gpu_layers.iter().max(),"tiles_built_max":p.tiles_built.iter().max(),"meets_60fps":intervals["throughput_fps"].as_f64().unwrap()>=59.5 && intervals["p95_ms"].as_f64().unwrap()<=16.9})
            }).collect();
            let viewport = self.app.canvas.viewport;
            let report = json!({"objects":100000,"completed_native_frames":self.frame,"egui_passes":ui.ctx().cumulative_pass_nr(),"build":"release","renderer":"wgpu","canvas_renderer":if self.persistent_gpu{"persistent_gpu_tiles"}else{"egui_mesh_tiles"},"adapter":self.adapter,"warmup_frames_per_phase":WARMUP,"sample_frames_per_phase":SAMPLES,"canvas_points":[viewport.width(),viewport.height()],"pixels_per_point":ui.ctx().pixels_per_point(),"phases":phases,"measurement":"native frame-start intervals; includes render/present waits and OS scheduling; not GPU timestamps or display scanout"});
            let result = serde_json::to_writer_pretty(&mut self.output, &report)
                .map_err(|e| e.to_string())
                .and_then(|()| self.output.write_all(b"\n").map_err(|e| e.to_string()))
                .and_then(|()| self.output.sync_all().map_err(|e| e.to_string()));
            *self.result.lock().unwrap() = Some(result);
            self.finished = true;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.frame > 0 && self.base.is_none() {
            self.base = Some(self.app.canvas.camera);
        }
        if let Some(base) = self.base {
            if local == 0 {
                self.app.canvas.camera = base;
            }
            let t = local as f64 / 60.0;
            self.app.canvas.camera.center = base.center;
            self.app.canvas.camera.pan_by_screen(vec2((t.sin() * 24.0) as f32, (t.cos() * 12.0) as f32));
            self.app.canvas.camera.zoom = base.zoom;
            let factor = match phase {
                1 => 1.0 + 0.2 * t.sin(),
                2 => 8.0,
                _ => 1.0,
            };
            let viewport = self.app.canvas.viewport;
            self.app.canvas.camera.zoom_about(viewport.center(), viewport, factor);
        }
        self.app.show(ui, frame.info().cpu_usage.map(|s| s * 1000.0));
        self.previous = Some((now, phase, local >= WARMUP, self.app.canvas.drawn, self.app.canvas.cache.stats));
        self.frame += 1;
        ui.ctx().request_repaint();
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        return Err("run this native performance example with --release".into());
    }
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("usage: native_benchmark OUTPUT.json (new file)")?);
    let persistent_gpu = match args.next() {
        None => true,
        Some(flag) if flag == "--reference" => false,
        Some(_) => return Err("usage: native_benchmark OUTPUT.json [--reference]".into()),
    };
    if args.next().is_some() {
        return Err("unexpected extra arguments".into());
    }
    let output = OpenOptions::new().write(true).create_new(true).open(&path)?;
    let result = Arc::new(Mutex::new(None));
    let shared = result.clone();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viamind — native 100k benchmark")
            .with_app_id("viamind-benchmark")
            .with_inner_size([1600.0, 1000.0])
            // Fixed size also requests an independent floating surface on tiling desktops.
            .with_min_inner_size([1600.0, 1000.0])
            .with_max_inner_size([1600.0, 1000.0])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native("Viamind benchmark",options,Box::new(move |cc| {
        let adapter=cc.wgpu_render_state.as_ref().map_or(Value::Null, |r| {let info=r.adapter.get_info();json!({"name":info.name,"backend":format!("{:?}",info.backend),"device_type":format!("{:?}",info.device_type),"driver":info.driver,"driver_info":info.driver_info})});
        let mut app=ViamindApp::with_scene_size(100000);
        if persistent_gpu {app.configure_renderer(cc); }
        Ok(Box::new(Benchmark { app, phases:std::array::from_fn(|_|Phase::default()), frame:0, base:None, last_native_frame:None, finished:false, previous:None, adapter, persistent_gpu, output, result:shared }))
    })).map_err(|e|e.to_string())?;
    let status = result.lock().unwrap().take().ok_or("benchmark closed before collecting all samples; output is incomplete")?;
    status.map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    println!("Native benchmark report: {}", path.display());
    Ok(())
}
