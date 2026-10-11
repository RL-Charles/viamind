//! Optional bounded observations of the legacy parity sequence; no camera/cache mutation.
use std::{
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

use egui::{ColorImage, Context, UserData};
use serde_json::{Value, json};
use viamind_ui_egui::{ViamindApp, scene::Prim};

const MAX_PIXELS: usize = 16 * 1024 * 1024;
const MAX_RAW_BYTES: usize = 640 * 1024 * 1024;
const MAX_METADATA: usize = 1024 * 1024;
const MAX_PASSES: usize = 1800;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Token {
    step: usize,
    sequence: usize,
}

struct Pending {
    token: Token,
    state: Value,
    draw_start: usize,
}

pub struct Trace {
    directory: PathBuf,
    started: Instant,
    initial_frame: Option<u64>,
    draws: Vec<Value>,
    images: Vec<Value>,
    pending: Option<Pending>,
    requests: usize,
    raw_bytes: usize,
    metadata_bytes: usize,
    issues: Vec<String>,
    finished: bool,
}

fn state(app: &ViamindApp, ctx: &Context) -> Value {
    let camera = app.canvas.camera;
    let rect = app.canvas.viewport;
    json!({
        "camera":{"center":[camera.center.x,camera.center.y],"zoom":camera.zoom},
        "viewport":[rect.min.x,rect.min.y,rect.max.x,rect.max.y],
        "pixels_per_point":ctx.pixels_per_point(),
        "layers":app.layers.draw_order().map(|l| json!({"id":l.id.0,"visible":l.visible,"opacity":l.opacity,"color":l.paint_color().to_array()})).collect::<Vec<_>>()
    })
}

fn frame(ctx: &Context) -> Value {
    json!({"frame":ctx.cumulative_frame_nr(),"pass":ctx.cumulative_pass_nr(),"pass_index":ctx.current_pass_index(),"viewport_id":format!("{:?}",ctx.viewport_id())})
}

fn token_json(token: Token) -> Value {
    json!({"case":token.step/2,"renderer":if token.step.is_multiple_of(2){"cached_cpu_mesh"}else{"persistent_gpu"},"step":token.step,"sequence":token.sequence})
}

fn raw_image_bytes(size: [usize; 2], actual_pixels: usize, prior: usize) -> Result<usize, String> {
    let pixels = size[0].checked_mul(size[1]).ok_or("diagnostic image extent overflow")?;
    let bytes = pixels.checked_mul(4).ok_or("diagnostic image byte overflow")?;
    if pixels == 0 || pixels > MAX_PIXELS || pixels != actual_pixels || prior.saturating_add(bytes) > MAX_RAW_BYTES {
        return Err("diagnostic raw image budget/extent mismatch".into());
    }
    Ok(bytes)
}

impl Trace {
    pub fn new(directory: PathBuf) -> Result<Self, String> {
        fs::create_dir(&directory).map_err(|e| format!("diagnostic directory must be NEW: {e}"))?;
        Ok(Self {
            directory,
            started: Instant::now(),
            initial_frame: None,
            draws: Vec::new(),
            images: Vec::new(),
            pending: None,
            requests: 0,
            raw_bytes: 0,
            metadata_bytes: 0,
            issues: Vec::new(),
            finished: false,
        })
    }

    pub fn check_budget(&mut self, ctx: &Context) -> Result<(), String> {
        let initial = *self.initial_frame.get_or_insert(ctx.cumulative_frame_nr());
        if ctx.cumulative_frame_nr().saturating_sub(initial) > 900 || self.started.elapsed() > Duration::from_secs(180) {
            return Err("diagnostic native frame/time budget exceeded".into());
        }
        Ok(())
    }

    pub fn draw(&mut self, step: usize, app: &ViamindApp, ctx: &Context) -> Result<(), String> {
        if self.draws.len() == MAX_PASSES {
            return Err("diagnostic UI-pass receipt budget exceeded".into());
        }
        let stats = app.canvas.cache.stats;
        self.draws.push(json!({"step":step,"frame":frame(ctx),"state":state(app,ctx),"drawn":app.canvas.drawn,
            "cache":{"tiles":stats.tiles,"tiles_drawn":stats.tiles_drawn,"tiles_built":stats.tiles_built,"vertices":stats.vertices,
                "gpu_layers":stats.gpu_layers,"gpu_draw_calls":stats.gpu_draw_calls}}));
        Ok(())
    }

    pub fn request(&mut self, step: usize, app: &ViamindApp, ctx: &Context) -> Result<UserData, String> {
        if self.pending.is_some() || self.requests == 10 {
            return Err("diagnostic request count/outstanding budget exceeded".into());
        }
        let token = Token { step, sequence: self.requests };
        self.requests += 1;
        self.pending =
            Some(Pending { token, state: json!({"frame":frame(ctx),"state":state(app,ctx)}), draw_start: self.draws.len().saturating_sub(1) });
        Ok(UserData::new(token))
    }

    pub fn image(
        &mut self,
        step: usize,
        viewport_id: egui::ViewportId,
        user_data: &UserData,
        image: &ColorImage,
        app: &ViamindApp,
        ctx: &Context,
    ) -> Result<(), String> {
        if self.images.len() == 10 {
            return Err("diagnostic image count exceeded".into());
        }
        let bytes = raw_image_bytes(image.size, image.pixels.len(), self.raw_bytes)?;
        self.raw_bytes += bytes;
        let supplied = user_data.data.as_ref().and_then(|d| d.downcast_ref::<Token>()).copied();
        let pending = self.pending.take();
        let arrival = json!({"frame":frame(ctx),"state":state(app,ctx),"legacy_step":step});
        let valid_token = viewport_id == ctx.viewport_id() && pending.as_ref().is_some_and(|p| Some(p.token) == supplied && p.token.step == step);
        if !valid_token {
            self.issues.push(format!("unmatched/duplicate screenshot token at legacy step {step}"));
        }
        let state_matches_request = pending.as_ref().is_some_and(|p| p.state["state"] == arrival["state"]);
        let draw_states_match_request =
            pending.as_ref().is_some_and(|p| self.draws[p.draw_start..].iter().all(|draw| draw["state"] == p.state["state"]));
        let pair_state_matches_reference = step.is_multiple_of(2)
            || self
                .images
                .last()
                .is_some_and(|previous| previous["arrival"]["state"] == arrival["state"] && previous["dimensions"] == json!(image.size));
        if !(state_matches_request && draw_states_match_request && pair_state_matches_reference) {
            self.issues.push(format!("state/dimension provenance differs at legacy step {step}"));
        }
        let file_name = format!("image-{:02}.p7", self.images.len());
        let file = OpenOptions::new().write(true).create_new(true).open(self.directory.join(&file_name)).map_err(|e| e.to_string())?;
        let mut writer = BufWriter::new(file);
        write!(writer, "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", image.size[0], image.size[1])
            .map_err(|e| e.to_string())?;
        for pixel in &image.pixels {
            writer.write_all(&pixel.to_array()).map_err(|e| e.to_string())?;
        }
        writer.flush().map_err(|e| e.to_string())?;
        writer.get_ref().sync_all().map_err(|e| e.to_string())?;
        let record = json!({"file":file_name,"dimensions":image.size,"token":supplied.map(token_json),"valid_token":valid_token,
            "state_matches_request":state_matches_request,"draw_states_match_request":draw_states_match_request,"pair_state_matches_reference":pair_state_matches_reference,
            "returned_viewport_id":format!("{viewport_id:?}"),
            "request":pending.as_ref().map(|p|&p.state),"arrival":arrival,
            "draw_receipts":pending.as_ref().map(|p| &self.draws[p.draw_start..]),
            "regions_at_arrival":regions(app,ctx),"original_rgba_bytes":bytes});
        self.write_json(&format!("image-{:02}.json", self.images.len()), &record)?;
        self.images.push(record);
        if !valid_token {
            return Err("invalid diagnostic token provenance; raw image retained".into());
        }
        Ok(())
    }

    fn write_json(&mut self, name: &str, value: &Value) -> Result<(), String> {
        let mut writer = CappedBytes { bytes: Vec::new(), maximum: MAX_METADATA.saturating_sub(self.metadata_bytes) };
        serde_json::to_writer_pretty(&mut writer, value).map_err(|e| e.to_string())?;
        let bytes = writer.bytes;
        self.metadata_bytes += bytes.len();
        let mut file = OpenOptions::new().write(true).create_new(true).open(self.directory.join(name)).map_err(|e| e.to_string())?;
        file.write_all(&bytes).and_then(|()| file.sync_all()).map_err(|e| e.to_string())
    }

    pub fn finish(&mut self, failure: Option<&str>) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        let report = json!({"mode":"initial_legacy_transition_trace","elapsed_seconds":self.started.elapsed().as_secs_f64(),
            "issues":self.issues,"failure":failure,"images":self.images.len(),"requests":self.requests,"raw_bytes":self.raw_bytes,
            "ui_pass_receipts":self.draws,"claim":"Observational legacy-sequence receipts only; valid paired state must be independently checked. No renderer correction or performance claim."});
        self.write_json("trace.json", &report)?;
        self.finished = true;
        if self.issues.is_empty() { Ok(()) } else { Err("legacy trace has invalid paired provenance; inspect retained receipts".into()) }
    }
}

struct CappedBytes {
    bytes: Vec<u8>,
    maximum: usize,
}
impl Write for CappedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.maximum {
            return Err(std::io::Error::other("diagnostic metadata budget exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn regions(app: &ViamindApp, ctx: &Context) -> Vec<Value> {
    // Added fixtures retain their original positions, including when outside the 8x view.
    app.scene
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let name = match item.prim {
                Prim::Via { .. } => "fixture_via_and_drill",
                Prim::Quad(_) => "fixture_quad",
                _ => return None,
            };
            let camera = app.canvas.camera;
            let viewport = app.canvas.viewport;
            let ppp = ctx.pixels_per_point();
            let min = camera.world_to_screen(item.bounds.min, viewport);
            let max = camera.world_to_screen(item.bounds.max, viewport);
            Some(json!({"name":name,"item":index,"screen_points":[min.x,min.y,max.x,max.y],
            "screen_pixels":[min.x*ppp,min.y*ppp,max.x*ppp,max.y*ppp],
            "intersects_canvas":egui::Rect::from_min_max(min,max).intersects(viewport)}))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_capture_refuses_overflow_extent_and_aggregate_before_copy() {
        assert_eq!(raw_image_bytes([4096, 4096], MAX_PIXELS, MAX_RAW_BYTES - 64 * 1024 * 1024).unwrap(), 64 * 1024 * 1024);
        assert!(raw_image_bytes([4096, 4096], MAX_PIXELS, MAX_RAW_BYTES - 64 * 1024 * 1024 + 1).is_err());
        assert!(raw_image_bytes([usize::MAX, 2], 0, 0).is_err());
        assert!(raw_image_bytes([4097, 4096], 4097 * 4096, 0).is_err());
        assert!(raw_image_bytes([8, 8], 63, 0).is_err());
        assert!(raw_image_bytes([0, 8], 0, 0).is_err());
    }

    #[test]
    fn metadata_limit_refuses_before_extending_and_new_destination_is_required() {
        let mut buffer = CappedBytes { bytes: Vec::new(), maximum: 3 };
        buffer.write_all(b"123").unwrap();
        assert!(buffer.write_all(b"4").is_err());
        assert_eq!(buffer.bytes, b"123");
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("new");
        let trace = Trace::new(path.clone()).unwrap();
        assert!(Trace::new(path).is_err());
        drop(trace);
    }
}
