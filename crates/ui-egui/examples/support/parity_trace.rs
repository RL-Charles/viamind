//! Optional bounded observations of the legacy parity sequence; no camera/cache mutation.
use std::{
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

use egui::{ColorImage, Context, Event, UserData, ViewportId};
use serde::Serialize;
use viamind_ui_egui::{ViamindApp, scene::Prim};

const MAX_PIXELS: usize = 16 * 1024 * 1024;
const MAX_RAW_BYTES: usize = 640 * 1024 * 1024;
const MAX_METADATA: usize = 1024 * 1024;
const MAX_PASSES: usize = 1800;
const MAX_LAYERS: usize = 64;
const MAX_REGIONS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Token {
    step: usize,
    sequence: usize,
}

#[derive(Default)]
struct MetadataBudget {
    used: usize,
}
impl MetadataBudget {
    fn reserve(&mut self, bytes: usize) -> Result<(), String> {
        let next = self.used.checked_add(bytes).ok_or("diagnostic metadata overflow")?;
        if next > MAX_METADATA {
            return Err("diagnostic aggregate metadata budget exceeded".into());
        }
        self.used = next;
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Serialize)]
struct LayerState {
    id: u16,
    visible: bool,
    opacity: f32,
    color: [u8; 4],
}
#[derive(PartialEq, Serialize)]
struct State {
    camera_center: [f64; 2],
    camera_zoom: f64,
    viewport: [f32; 4],
    pixels_per_point: f32,
    layers: Vec<LayerState>,
}
impl State {
    fn capture(app: &ViamindApp, ctx: &Context, budget: &mut MetadataBudget) -> Result<Self, String> {
        let count = app.layers.draw_order().count();
        if count > MAX_LAYERS {
            return Err("diagnostic layer metadata count exceeded".into());
        }
        // Charge heap capacity BEFORE collecting any rows. Struct slots are precharged by Trace.
        budget.reserve(count * size_of::<LayerState>())?;
        let mut layers = Vec::with_capacity(count);
        layers.extend(app.layers.draw_order().map(|l| LayerState {
            id: l.id.0,
            visible: l.visible,
            opacity: l.opacity,
            color: l.paint_color().to_array(),
        }));
        let camera = app.canvas.camera;
        let rect = app.canvas.viewport;
        Ok(Self {
            camera_center: [camera.center.x, camera.center.y],
            camera_zoom: camera.zoom,
            viewport: [rect.min.x, rect.min.y, rect.max.x, rect.max.y],
            pixels_per_point: ctx.pixels_per_point(),
            layers,
        })
    }
}
#[derive(Clone, Copy, Serialize)]
struct Frame {
    frame: u64,
    pass: u64,
    pass_index: usize,
}
fn frame(ctx: &Context) -> Frame {
    Frame { frame: ctx.cumulative_frame_nr(), pass: ctx.cumulative_pass_nr(), pass_index: ctx.current_pass_index() }
}
#[derive(Serialize)]
struct Draw {
    step: usize,
    frame: Frame,
    state: State,
    drawn: usize,
    cache: [usize; 6],
}
#[derive(Serialize)]
struct Pending {
    token: Token,
    frame: Frame,
    state: State,
    draw_start: usize,
}
#[derive(Serialize)]
struct Region {
    name: &'static str,
    item: usize,
    screen_pixels: [f32; 4],
    intersects_canvas: bool,
}
#[derive(Serialize)]
struct ImageReceipt {
    image: usize,
    dimensions: [usize; 2],
    token: Option<Token>,
    valid_token: bool,
    viewport_matches: bool,
    state_matches_request: bool,
    draw_states_match_request: bool,
    pair_state_matches_reference: bool,
    pair_dimensions_match_reference: bool,
    request: Option<Pending>,
    arrival: Frame,
    arrival_state: State,
    legacy_step: usize,
    draw_end: usize,
    regions: Vec<Region>,
    original_rgba_bytes: usize,
}
#[derive(Serialize)]
struct Report<'a> {
    mode: &'static str,
    elapsed_seconds: f64,
    failure: Option<&'a str>,
    requests: usize,
    raw_bytes: usize,
    metadata_charged: usize,
    paired_provenance_valid: bool,
    images: &'a [ImageReceipt],
    ui_pass_receipts: &'a [Draw],
    claim: &'static str,
}

pub struct Trace {
    directory: PathBuf,
    started: Instant,
    initial_frame: Option<u64>,
    draws: Vec<Draw>,
    images: Vec<ImageReceipt>,
    pending: Option<Pending>,
    requests: usize,
    raw_bytes: usize,
    metadata: MetadataBudget,
    finished: bool,
}

fn raw_image_bytes(size: [usize; 2], actual_pixels: usize, prior: usize) -> Result<usize, String> {
    let pixels = size[0].checked_mul(size[1]).ok_or("diagnostic image extent overflow")?;
    let bytes = pixels.checked_mul(4).ok_or("diagnostic image byte overflow")?;
    if pixels == 0 || pixels > MAX_PIXELS || pixels != actual_pixels || prior.saturating_add(bytes) > MAX_RAW_BYTES {
        return Err("diagnostic raw image budget/extent mismatch".into());
    }
    Ok(bytes)
}

/// Count all events before cloning an image or examining token data. Initial trace never
/// silently discards a second readback from the same input, including repeated-pass input.
pub fn screenshot(events: &[Event]) -> Result<Option<(std::sync::Arc<ColorImage>, UserData, ViewportId)>, String> {
    if events.iter().filter(|e| matches!(e, Event::Screenshot { .. })).take(2).count() > 1 {
        return Err("unexpected extra Screenshot events in one input".into());
    }
    Ok(events.iter().find_map(|event| match event {
        Event::Screenshot { image, user_data, viewport_id } => Some((std::sync::Arc::clone(image), user_data.clone(), *viewport_id)),
        _ => None,
    }))
}

impl Trace {
    pub fn new(directory: PathBuf) -> Result<Self, String> {
        if directory.as_os_str().len() > 4096 {
            return Err("diagnostic output path exceeds 4096 bytes".into());
        }
        let mut metadata = MetadataBudget::default();
        // Preallocate bounded, lean typed slots once. No retained JSON trees or report clones.
        // Fixed allowance covers one I/O buffer, bounded paths/tokens and an 8KiB failure report.
        metadata.reserve(MAX_PASSES * size_of::<Draw>() + 10 * size_of::<ImageReceipt>() + size_of::<Pending>() + size_of::<State>() + 64 * 1024)?;
        fs::create_dir(&directory).map_err(|e| format!("diagnostic directory must be NEW: {e}"))?;
        Ok(Self {
            directory,
            started: Instant::now(),
            initial_frame: None,
            draws: Vec::with_capacity(MAX_PASSES),
            images: Vec::with_capacity(10),
            pending: None,
            requests: 0,
            raw_bytes: 0,
            metadata,
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
        let state = State::capture(app, ctx, &mut self.metadata)?;
        let s = app.canvas.cache.stats;
        self.draws.push(Draw {
            step,
            frame: frame(ctx),
            state,
            drawn: app.canvas.drawn,
            cache: [s.tiles, s.tiles_drawn, s.tiles_built, s.vertices, s.gpu_layers, s.gpu_draw_calls],
        });
        Ok(())
    }

    pub fn request(&mut self, step: usize, app: &ViamindApp, ctx: &Context) -> Result<UserData, String> {
        if self.pending.is_some() || self.requests == 10 {
            return Err("diagnostic request count/outstanding budget exceeded".into());
        }
        let state = State::capture(app, ctx, &mut self.metadata)?;
        let token = Token { step, sequence: self.requests };
        self.requests += 1;
        self.pending = Some(Pending { token, frame: frame(ctx), state, draw_start: self.draws.len().saturating_sub(1) });
        Ok(UserData::new(token))
    }

    pub fn image(
        &mut self,
        step: usize,
        viewport_id: ViewportId,
        user_data: &UserData,
        image: &ColorImage,
        app: &ViamindApp,
        ctx: &Context,
    ) -> Result<(), String> {
        if self.images.len() == 10 {
            return Err("diagnostic image count exceeded".into());
        }
        let bytes = raw_image_bytes(image.size, image.pixels.len(), self.raw_bytes)?;
        let state = State::capture(app, ctx, &mut self.metadata)?;
        let regions = regions(app, ctx, &mut self.metadata)?;
        let supplied = user_data.data.as_ref().and_then(|d| d.downcast_ref::<Token>()).copied();
        let pending = self.pending.take();
        let viewport_matches = viewport_id == ctx.viewport_id();
        let valid_token = viewport_matches && pending.as_ref().is_some_and(|p| Some(p.token) == supplied && p.token.step == step);
        let state_matches_request = pending.as_ref().is_some_and(|p| p.state == state);
        let draw_states_match_request = pending.as_ref().is_some_and(|p| self.draws[p.draw_start..].iter().all(|draw| draw.state == p.state));
        let pair_state_matches_reference =
            step.is_multiple_of(2) || self.images.last().is_some_and(|previous| previous.arrival_state == state && previous.dimensions == image.size);
        let pair_dimensions_match_reference = step.is_multiple_of(2) || self.images.last().is_some_and(|previous| previous.dimensions == image.size);
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
        drop(writer);
        self.raw_bytes += bytes;
        self.images.push(ImageReceipt {
            image: self.images.len(),
            dimensions: image.size,
            token: supplied,
            valid_token,
            viewport_matches,
            state_matches_request,
            draw_states_match_request,
            pair_state_matches_reference,
            pair_dimensions_match_reference,
            request: pending,
            arrival: frame(ctx),
            arrival_state: state,
            legacy_step: step,
            draw_end: self.draws.len(),
            regions,
            original_rgba_bytes: bytes,
        });
        let index = self.images.len() - 1;
        write_json(&self.directory.join(format!("image-{index:02}.json")), &self.images[index], &mut self.metadata)?;
        if !valid_token {
            return Err("invalid diagnostic token provenance; raw image retained".into());
        }
        // Legacy comparison indexes the reference using the arriving image bounds. Refuse
        // incompatible extents after retaining both receipts, before those indices are used.
        if !pair_dimensions_match_reference {
            return Err("diagnostic pair dimensions differ; raw images retained before comparison".into());
        }
        Ok(())
    }

    pub fn finish(&mut self, failure: Option<&str>) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        let valid = self.images.len() == 10
            && self.requests == 10
            && self.images.iter().all(|r| r.valid_token && r.state_matches_request && r.draw_states_match_request && r.pair_state_matches_reference);
        let report = Report {
            mode: "initial_legacy_transition_trace",
            elapsed_seconds: self.started.elapsed().as_secs_f64(),
            failure,
            requests: self.requests,
            raw_bytes: self.raw_bytes,
            metadata_charged: self.metadata.used,
            paired_provenance_valid: valid,
            images: &self.images,
            ui_pass_receipts: &self.draws,
            claim: "Observational legacy-sequence receipts only. Invalid provenance is not valid pixel parity. No renderer correction or performance claim.",
        };
        if let Err(error) = write_json(&self.directory.join("trace.json"), &report, &mut self.metadata) {
            // The fixed precharged allowance retains an actionable bounded failure even when
            // normal metadata is exhausted. No report JSON is cloned or materialized in memory.
            #[derive(Serialize)]
            struct Failure<'a> {
                failure: &'a str,
                images: usize,
                requests: usize,
                metadata_charged: usize,
            }
            let reason: String = error.chars().take(256).collect();
            let failure = Failure { failure: &reason, images: self.images.len(), requests: self.requests, metadata_charged: self.metadata.used };
            let mut reserved = MetadataBudget { used: MAX_METADATA - 8192 };
            let _ = write_json(&self.directory.join("failure.json"), &failure, &mut reserved);
            self.finished = true;
            return Err(error);
        }
        self.finished = true;
        if valid { Ok(()) } else { Err("legacy trace has invalid paired provenance; inspect retained receipts".into()) }
    }
}

struct LimitedWriter<'a, W> {
    inner: W,
    budget: &'a mut MetadataBudget,
}
impl<W: Write> Write for LimitedWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.budget.reserve(bytes.len()).map_err(std::io::Error::other)?;
        self.inner.write_all(bytes)?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
fn write_json(path: &std::path::Path, value: &impl Serialize, budget: &mut MetadataBudget) -> Result<(), String> {
    let file = OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| e.to_string())?;
    let mut writer = LimitedWriter { inner: BufWriter::new(file), budget };
    serde_json::to_writer_pretty(&mut writer, value).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;
    writer.inner.get_ref().sync_all().map_err(|e| e.to_string())
}

fn regions(app: &ViamindApp, ctx: &Context, budget: &mut MetadataBudget) -> Result<Vec<Region>, String> {
    let count = app.scene.items.iter().filter(|item| matches!(item.prim, Prim::Via { .. } | Prim::Quad(_))).count();
    if count > MAX_REGIONS {
        return Err("diagnostic fixture region count exceeded".into());
    }
    budget.reserve(count * size_of::<Region>())?;
    let mut result = Vec::with_capacity(count);
    for (index, item) in app.scene.items.iter().enumerate() {
        let name = match item.prim {
            Prim::Via { .. } => "fixture_via_and_drill",
            Prim::Quad(_) => "fixture_quad",
            _ => continue,
        };
        let camera = app.canvas.camera;
        let viewport = app.canvas.viewport;
        let ppp = ctx.pixels_per_point();
        let min = camera.world_to_screen(item.bounds.min, viewport);
        let max = camera.world_to_screen(item.bounds.max, viewport);
        result.push(Region {
            name,
            item: index,
            screen_pixels: [min.x * ppp, min.y * ppp, max.x * ppp, max.y * ppp],
            intersects_canvas: egui::Rect::from_min_max(min, max).intersects(viewport),
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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
    fn cumulative_metadata_refuses_before_retaining_or_serializing_more() {
        let mut budget = MetadataBudget { used: MAX_METADATA - 3 };
        let mut buffer = Vec::new();
        let mut writer = LimitedWriter { inner: &mut buffer, budget: &mut budget };
        writer.write_all(b"123").unwrap();
        assert!(writer.write_all(b"4").is_err());
        assert_eq!(buffer, b"123");
        assert_eq!(budget.used, MAX_METADATA);
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("new");
        let mut trace = Trace::new(path.clone()).unwrap();
        assert!(Trace::new(path).is_err());
        let ctx = Context::default();
        let app = ViamindApp::with_scene_size(1);
        trace.metadata.used = MAX_METADATA;
        assert!(trace.draw(0, &app, &ctx).is_err());
        assert!(trace.draws.is_empty());
        assert_eq!(trace.metadata.used, MAX_METADATA);
    }

    #[test]
    fn multiple_screenshots_and_repeated_pass_token_cannot_be_silently_accepted() {
        let image = Arc::new(ColorImage::filled([1, 1], egui::Color32::WHITE));
        let event =
            Event::Screenshot { viewport_id: ViewportId::ROOT, user_data: UserData::new(Token { step: 0, sequence: 0 }), image: Arc::clone(&image) };
        assert!(screenshot(&[event.clone(), event]).is_err());
        let temporary = tempfile::tempdir().unwrap();
        let mut trace = Trace::new(temporary.path().join("new")).unwrap();
        let ctx = Context::default();
        let mut app = ViamindApp::with_scene_size(1);
        app.canvas.viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1.0, 1.0));
        trace.draw(0, &app, &ctx).unwrap();
        let token = trace.request(0, &app, &ctx).unwrap();
        trace.image(0, ViewportId::ROOT, &token, &image, &app, &ctx).unwrap();
        // Same event replayed by a second UI pass: no outstanding request remains.
        assert!(trace.image(1, ViewportId::ROOT, &token, &image, &app, &ctx).is_err());
        assert!(!trace.images[1].valid_token);
        assert_eq!(trace.requests, 1);
    }

    #[test]
    fn mismatched_pair_extents_are_retained_and_refused_before_legacy_indexing() {
        for (reference_size, gpu_size) in [([1, 1], [2, 2]), ([2, 2], [1, 1])] {
            let temporary = tempfile::tempdir().unwrap();
            let directory = temporary.path().join("new");
            let mut trace = Trace::new(directory.clone()).unwrap();
            let ctx = Context::default();
            let app = ViamindApp::with_scene_size(1);
            trace.draw(0, &app, &ctx).unwrap();
            let token = trace.request(0, &app, &ctx).unwrap();
            let reference = ColorImage::filled(reference_size, egui::Color32::WHITE);
            trace.image(0, ViewportId::ROOT, &token, &reference, &app, &ctx).unwrap();
            trace.draw(1, &app, &ctx).unwrap();
            let token = trace.request(1, &app, &ctx).unwrap();
            let gpu = ColorImage::filled(gpu_size, egui::Color32::WHITE);
            let error = trace.image(1, ViewportId::ROOT, &token, &gpu, &app, &ctx).unwrap_err();
            assert!(error.contains("pair dimensions differ"));
            assert_eq!(trace.images.len(), 2);
            assert!(trace.images[1].valid_token);
            assert!(!trace.images[1].pair_dimensions_match_reference);
            assert!(directory.join("image-00.p7").is_file());
            assert!(directory.join("image-01.p7").is_file());
            assert!(directory.join("image-01.json").is_file());
            assert!(trace.finish(Some(&error)).is_err());
            assert!(directory.join("trace.json").is_file());
        }
    }
}
