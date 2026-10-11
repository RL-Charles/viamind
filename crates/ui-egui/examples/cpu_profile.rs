//! Stage timing of the production CPU canvas and egui tessellator; release only.
use egui::{RawInput, Rect, pos2, vec2};
use serde_json::json;
use std::time::Instant;
use viamind_ui_egui::ViamindApp;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        return Err("use --release".into());
    }
    let ctx = egui::Context::default();
    let mut app = ViamindApp::with_scene_size(100_000);
    let mut ui_ms = Vec::new();
    let mut tess_ms = Vec::new();
    let mut vertices = 0;
    let mut indices = 0;
    for frame in 0..180 {
        if frame > 0 {
            app.canvas.camera.pan_by_screen(vec2(0.25, 0.125));
        }
        let start = Instant::now();
        let mut output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1600.0, 1000.0))),
                time: Some(f64::from(frame) / 60.0),
                ..Default::default()
            },
            |ui| app.show(ui, None),
        );
        output.textures_delta.clear(); // Stage profile deliberately excludes texture upload.
        let ui_elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let tess_elapsed = start.elapsed().as_secs_f64() * 1000.0;
        vertices = 0;
        indices = 0;
        for p in primitives {
            if let egui::epaint::Primitive::Mesh(m) = p.primitive {
                vertices += m.vertices.len();
                indices += m.indices.len();
            }
        }
        if frame >= 60 {
            ui_ms.push(ui_elapsed);
            tess_ms.push(tess_elapsed);
        }
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    println!(
        "{}",
        json!({"objects":100000,"drawn":app.canvas.drawn,"canvas_points":[app.canvas.viewport.width(),app.canvas.viewport.height()],"sample_frames":ui_ms.len(),"mean_ui_ms":mean(&ui_ms),"mean_tessellation_ms":mean(&tess_ms),"vertices":vertices,"indices":indices,"mesh_upload_bytes_per_frame":vertices*20+indices*4,"measurement":"CPU UI+egui tessellation only; excludes GPU upload/paint/present"})
    );
    Ok(())
}
