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
use viamind_ui_egui::{
    ViamindApp,
    camera::Camera,
    gpu_timing::{FrameTag, GpuTiming},
};

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
    gpu_timing: Option<Arc<GpuTiming>>,
    profiling: bool,
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
            let report = json!({"objects":100000,"completed_native_frames":self.frame,"egui_passes":ui.ctx().cumulative_pass_nr(),"build":"release","renderer":"wgpu","canvas_renderer":if self.persistent_gpu && self.app.canvas.cache.stats.gpu_layers > 0 {"persistent_gpu_layers"}else{"egui_mesh_tiles"},"adapter":self.adapter,"warmup_frames_per_phase":WARMUP,"sample_frames_per_phase":SAMPLES,"canvas_points":[viewport.width(),viewport.height()],"pixels_per_point":ui.ctx().pixels_per_point(),"phases":phases,"gpu_timestamp_instrumentation":self.profiling,"measurement":"native frame-start intervals; includes render/present waits and OS scheduling; not GPU timestamps or display scanout"});
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
        if let Some(timing) = &self.gpu_timing {
            timing.set_frame(FrameTag { frame: self.frame as u64, phase, sampled: local >= WARMUP });
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
    let mut persistent_gpu = true;
    let mut profile_path = None;
    while let Some(flag) = args.next() {
        if flag == "--reference" {
            persistent_gpu = false;
        } else if flag == "--gpu-profile" && profile_path.is_none() {
            profile_path = Some(PathBuf::from(args.next().ok_or("--gpu-profile requires a new JSON path")?));
        } else {
            return Err("usage: native_benchmark OUTPUT.json [--reference | --gpu-profile GPU.json]".into());
        }
    }
    if !persistent_gpu && profile_path.is_some() {
        return Err("GPU callback profiling requires the persistent GPU path".into());
    }
    let profile_file = profile_path.map(|p| OpenOptions::new().write(true).create_new(true).open(p)).transpose()?;
    let profiling = profile_file.is_some();
    let timing_state: Arc<Mutex<Option<Arc<GpuTiming>>>> = Arc::new(Mutex::new(None));
    let timing_shared = Arc::clone(&timing_state);
    let adapter_state = Arc::new(Mutex::new(Value::Null));
    let adapter_shared = Arc::clone(&adapter_state);
    let output = OpenOptions::new().write(true).create_new(true).open(&path)?;
    let result = Arc::new(Mutex::new(None));
    let shared = result.clone();
    let mut options = eframe::NativeOptions {
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
    if profiling && let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        let base = Arc::clone(&setup.device_descriptor);
        setup.device_descriptor = Arc::new(move |adapter| {
            let mut descriptor = base(adapter);
            if adapter.features().contains(GpuTiming::required_features()) {
                descriptor.required_features |= GpuTiming::required_features();
            }
            descriptor
        });
    }
    eframe::run_native("Viamind benchmark",options,Box::new(move |cc| {
        let adapter=cc.wgpu_render_state.as_ref().map_or(Value::Null, |r| {let info=r.adapter.get_info();json!({"name":info.name,"backend":format!("{:?}",info.backend),"device_type":format!("{:?}",info.device_type),"driver":info.driver,"driver_info":info.driver_info})});
        *adapter_shared.lock().unwrap() = adapter.clone();
        let mut app=ViamindApp::with_scene_size(100000);
        if persistent_gpu {app.configure_renderer(cc); }
        let gpu_timing = if profiling { cc.wgpu_render_state.as_ref().and_then(|r| GpuTiming::new(r.device.clone(),r.queue.clone())) } else { None };
        app.canvas.cache.set_gpu_timing(gpu_timing.clone());
        *timing_shared.lock().unwrap() = gpu_timing.clone();
        Ok(Box::new(Benchmark { app, phases:std::array::from_fn(|_|Phase::default()), frame:0, base:None, last_native_frame:None, finished:false, previous:None, adapter, persistent_gpu, gpu_timing, profiling, output, result:shared }))
    })).map_err(|e|e.to_string())?;
    let status = result.lock().unwrap().take().ok_or("benchmark closed before collecting all samples; output is incomplete")?;
    status.map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    if let Some(mut file) = profile_file {
        let timing = timing_state.lock().unwrap().take();
        let mut failed = false;
        let mut report = if let Some(timing) = timing {
            let finish_error = timing.finish().err();
            let report = timing.report();
            let phases: Vec<_> = PHASES
                .iter()
                .enumerate()
                .map(|(phase, name)| {
                    let values: Vec<_> = report.samples.iter().filter(|s| s.tag.sampled && s.tag.phase == phase).map(|s| s.gpu_ms).collect();
                    let timing = if values.is_empty() {
                        Value::Null
                    } else {
                        let mut value = stats(&values);
                        value.as_object_mut().unwrap().remove("throughput_fps");
                        value
                    };
                    json!({"name":name,"gpu_board_commands":timing})
                })
                .collect();
            failed = finish_error.is_some()
                || !report.errors.is_empty()
                || report.pending_slots > 0
                || phases.iter().any(|p| p["gpu_board_commands"].is_null());
            json!({"status":if failed{"incomplete"}else{"complete"},"source":option_env!("VIAMIND_BUILD_COMMIT"),"phases":phases,"readback":report,"finish_error":finish_error,"measurement":"GPU timestamps around production board draw commands only; excludes UI, background, overlays, uploads, present waits and CPU scheduling. Instrumented cadence is diagnostic; use an ordinary run for acceptance."})
        } else {
            json!({"status":"unavailable","reason":"Device did not enable timestamp queries inside render passes; cadence still collected."})
        };
        report["adapter"] = adapter_state.lock().unwrap().clone();
        report["cadence_report"] = json!(path);
        report["source"] = json!(option_env!("VIAMIND_BUILD_COMMIT"));
        report["expected_sample_frames_per_phase"] = json!(SAMPLES);
        serde_json::to_writer_pretty(&mut file, &report)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        if failed {
            return Err("GPU timing diagnostic incomplete; inspect its report".into());
        }
    }
    println!("Native benchmark report: {}", path.display());
    Ok(())
}
