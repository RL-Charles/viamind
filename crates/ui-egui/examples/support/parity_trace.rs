//! Optional bounded observations of the legacy parity sequence; no camera/cache mutation.
use std::{
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::parity_progress;
use egui::{ColorImage, Context, Event, UserData, ViewportId};
use serde::Serialize;
use viamind_ui_egui::{ViamindApp, scene::Prim};

const MAX_PIXELS: usize = 16 * 1024 * 1024;
const MAX_RAW_BYTES: usize = 640 * 1024 * 1024;
const MAX_METADATA: usize = 1024 * 1024;
const MAX_PASSES: usize = 1800;
const MAX_LAYERS: usize = 64;
const MAX_REGIONS: usize = 2;
const MAX_INPUT_EVENTS: usize = 100_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum IoMode {
    Immediate,
    Deferred,
}

#[derive(Debug, Default, Serialize)]
struct NavigationInput {
    pointer_motion: usize,
    pointer_button: usize,
    wheel: usize,
    zoom: usize,
    home: usize,
    space: usize,
    smooth_scroll: [f32; 2],
    zoom_delta: f32,
    pointer_delta: [f32; 2],
    home_held: bool,
    space_held: bool,
    middle_held: bool,
}
fn input_count(ctx: &Context) -> Result<(), String> {
    if ctx.input(|i| i.events.len()) > MAX_INPUT_EVENTS { Err("diagnostic input event count exceeded".into()) } else { Ok(()) }
}
fn navigation_input(ctx: &Context) -> Result<NavigationInput, String> {
    input_count(ctx)?;
    Ok(ctx.input(|i| {
        let mut result = NavigationInput {
            smooth_scroll: [i.smooth_scroll_delta.x, i.smooth_scroll_delta.y],
            zoom_delta: i.zoom_delta(),
            pointer_delta: [i.pointer.delta().x, i.pointer.delta().y],
            home_held: i.key_down(egui::Key::Home),
            space_held: i.key_down(egui::Key::Space),
            middle_held: i.pointer.button_down(egui::PointerButton::Middle),
            ..Default::default()
        };
        for event in &i.events {
            match event {
                Event::PointerMoved(_) => result.pointer_motion += 1,
                Event::PointerButton { .. } => result.pointer_button += 1,
                Event::MouseWheel { .. } => result.wheel += 1,
                Event::Zoom(_) => result.zoom += 1,
                Event::Key { key: egui::Key::Home, .. } => result.home += 1,
                Event::Key { key: egui::Key::Space, .. } => result.space += 1,
                _ => {}
            }
        }
        result
    }))
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    navigation_input: Option<NavigationInput>,
}
#[derive(Serialize)]
struct Pending {
    token: Token,
    frame: Frame,
    state: State,
    draw_start: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    completed_stable_frames: Option<usize>,
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
    io_mode: IoMode,
    deferred_images: Vec<Option<std::sync::Arc<ColorImage>>>,
    finished: bool,
    progress: Option<parity_progress::Controller>,
    hardened: bool,
    settling: Option<super::parity_admission::Settling>,
    settled_draw: Option<usize>,
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
        Self::with_mode(directory, IoMode::Immediate)
    }

    pub fn deferred(directory: PathBuf) -> Result<Self, String> {
        Self::with_mode(directory, IoMode::Deferred)
    }

    fn with_mode(directory: PathBuf, io_mode: IoMode) -> Result<Self, String> {
        if directory.as_os_str().len() > 4096 {
            return Err("diagnostic output path exceeds 4096 bytes".into());
        }
        let mut metadata = MetadataBudget::default();
        // Preallocate bounded, lean typed slots once. No retained JSON trees or report clones.
        // Fixed allowance covers one I/O buffer, bounded paths/tokens and an 8KiB failure report.
        metadata.reserve(
            MAX_PASSES * size_of::<Draw>()
                + 10 * size_of::<ImageReceipt>()
                + size_of::<Pending>()
                + size_of::<State>()
                + 64 * 1024
                + if io_mode == IoMode::Deferred { 10 * size_of::<Option<std::sync::Arc<ColorImage>>>() } else { 0 },
        )?;
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
            io_mode,
            deferred_images: Vec::with_capacity(if io_mode == IoMode::Deferred { 10 } else { 0 }),
            finished: false,
            progress: None,
            hardened: false,
            settling: None,
            settled_draw: None,
        })
    }

    pub fn enable_hardened(&mut self) -> Result<(), String> {
        if self.io_mode != IoMode::Deferred || self.hardened {
            return Err("hardened admission requires one deferred trace".into());
        }
        self.metadata.reserve(size_of::<super::parity_admission::Settling>() + size_of::<Option<usize>>() + size_of::<bool>())?;
        self.hardened = true;
        self.settling = Some(super::parity_admission::Settling::default());
        Ok(())
    }

    pub fn check_hardened_input(&self, ctx: &Context) -> Result<(), String> {
        let input = navigation_input(ctx)?;
        if input.wheel != 0
            || input.zoom != 0
            || input.smooth_scroll != [0.0; 2]
            || input.zoom_delta != 1.0
            || input.home != 0
            || input.home_held
            || input.middle_held
            || (input.space_held && input.pointer_delta != [0.0; 2])
        {
            return Err("hardened parity refused external navigation input".into());
        }
        Ok(())
    }

    pub fn hardened_ready(&mut self, step: usize, ctx: &Context) -> Result<(bool, usize), String> {
        let index = self.draws.len().checked_sub(1).ok_or("hardened settling requires a drawn frame")?;
        let current = &self.draws[index];
        let unchanged = self.settled_draw.is_none_or(|previous| self.draws[previous].state == current.state);
        let settling = self.settling.as_mut().ok_or("hardened settling not admitted")?;
        let ready = settling.observe(current.frame.frame, step, unchanged, ctx.will_discard()).map_err(str::to_owned)?;
        self.settled_draw = Some(index);
        Ok((ready, settling.completed))
    }

    pub fn enable_progress(&mut self) -> Result<parity_progress::Worker, String> {
        if self.io_mode != IoMode::Deferred || self.progress.is_some() {
            return Err("progress watchdog requires one deferred trace".into());
        }
        self.metadata.reserve(parity_progress::BUDGET_BYTES)?;
        let (controller, worker) = parity_progress::start(self.directory.join("progress.json"), Duration::from_secs(180))?;
        self.progress = Some(controller);
        Ok(worker)
    }

    pub fn progress(&self) -> Option<std::sync::Arc<parity_progress::Progress>> {
        self.progress.as_ref().map(|controller| std::sync::Arc::clone(&controller.progress))
    }

    pub fn check_budget(&mut self, ctx: &Context) -> Result<(), String> {
        if self.io_mode == IoMode::Deferred {
            input_count(ctx)?;
        }
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
        let navigation_input = if self.io_mode == IoMode::Deferred { Some(navigation_input(ctx)?) } else { None };
        let s = app.canvas.cache.stats;
        let observed_frame = frame(ctx);
        if let Some(controller) = &self.progress {
            controller.progress.observed_frame(observed_frame.frame, observed_frame.pass);
        }
        self.draws.push(Draw {
            step,
            frame: observed_frame,
            state,
            drawn: app.canvas.drawn,
            cache: [s.tiles, s.tiles_drawn, s.tiles_built, s.vertices, s.gpu_layers, s.gpu_draw_calls],
            navigation_input,
        });
        Ok(())
    }

    pub fn request(&mut self, step: usize, app: &ViamindApp, ctx: &Context) -> Result<UserData, String> {
        if self.pending.is_some() || self.requests == 10 {
            return Err("diagnostic request count/outstanding budget exceeded".into());
        }
        let state = State::capture(app, ctx, &mut self.metadata)?;
        let token = Token { step, sequence: self.requests };
        if let Some(controller) = &self.progress {
            controller.progress.prepared(token.sequence);
        }
        self.requests += 1;
        self.pending = Some(Pending {
            token,
            frame: frame(ctx),
            state,
            draw_start: self.draws.len().saturating_sub(1),
            completed_stable_frames: self.settling.as_ref().map(|settling| settling.completed),
        });
        Ok(UserData::new(token))
    }

    pub fn image(
        &mut self,
        step: usize,
        viewport_id: ViewportId,
        user_data: &UserData,
        image: &std::sync::Arc<ColorImage>,
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
        let viewport_matches = viewport_id == ctx.viewport_id() && (!self.hardened || viewport_id == ViewportId::ROOT);
        let valid_token = viewport_matches && pending.as_ref().is_some_and(|p| Some(p.token) == supplied && p.token.step == step);
        let state_matches_request = pending.as_ref().is_some_and(|p| p.state == state);
        let draw_states_match_request = pending.as_ref().is_some_and(|p| self.draws[p.draw_start..].iter().all(|draw| draw.state == p.state));
        let pair_state_matches_reference =
            step.is_multiple_of(2) || self.images.last().is_some_and(|previous| previous.arrival_state == state && previous.dimensions == image.size);
        let pair_dimensions_match_reference = step.is_multiple_of(2) || self.images.last().is_some_and(|previous| previous.dimensions == image.size);
        if self.io_mode == IoMode::Deferred {
            // No pixel copy and no image/JSON I/O until the sequence is terminal. The checked
            // raw aggregate above admits this unique image before retaining its shared handle.
            self.deferred_images.push(Some(std::sync::Arc::clone(image)));
        } else {
            write_image(&self.directory, self.images.len(), image)?;
        }
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
        if self.io_mode == IoMode::Immediate {
            write_json(&self.directory.join(format!("image-{index:02}.json")), &self.images[index], &mut self.metadata)?;
        }
        if !valid_token {
            return Err("invalid diagnostic token provenance; raw image retained".into());
        }
        // Legacy comparison indexes the reference using the arriving image bounds. Refuse
        // incompatible extents after retaining both receipts, before those indices are used.
        if !pair_dimensions_match_reference {
            return Err("diagnostic pair dimensions differ; raw images retained before comparison".into());
        }
        if self.hardened && (!state_matches_request || !draw_states_match_request || !pair_state_matches_reference) {
            return Err("hardened pair state differs from request, intervening draw or reference; raw images retained".into());
        }
        Ok(())
    }

    pub fn finish(&mut self, failure: Option<&str>) -> Result<(), String> {
        if let Some(controller) = &self.progress {
            controller.progress.phase(parity_progress::Phase::Flushing);
        }
        let result = self.finish_inner(failure);
        if let Some(controller) = &mut self.progress {
            controller.terminal(failure.is_none() && result.is_ok());
        }
        result
    }

    fn finish_inner(&mut self, failure: Option<&str>) -> Result<(), String> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        let flush_error = self.flush_deferred().err();
        let failure = flush_error.as_deref().or(failure);
        let valid = self.images.len() == 10
            && self.requests == 10
            && self.images.iter().all(|r| r.valid_token && r.state_matches_request && r.draw_states_match_request && r.pair_state_matches_reference);
        let report = Report {
            mode: if self.hardened {
                "hardened_manual_camera_completed_frame_trace"
            } else if self.io_mode == IoMode::Deferred {
                "deferred_io_legacy_transition_trace"
            } else {
                "initial_legacy_transition_trace"
            },
            elapsed_seconds: self.started.elapsed().as_secs_f64(),
            failure,
            requests: self.requests,
            raw_bytes: self.raw_bytes,
            metadata_charged: self.metadata.used,
            paired_provenance_valid: valid,
            images: &self.images,
            ui_pass_receipts: &self.draws,
            claim: "Harness receipts only. Invalid provenance is not valid pixel parity. No historical cause, renderer correction or performance claim.",
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
        if let Some(error) = flush_error {
            Err(error)
        } else if valid {
            Ok(())
        } else {
            Err("legacy trace has invalid paired provenance; inspect retained receipts".into())
        }
    }

    fn flush_deferred(&mut self) -> Result<(), String> {
        for (index, slot) in self.deferred_images.iter_mut().enumerate() {
            let Some(image) = slot.take() else { continue };
            write_image(&self.directory, index, &image)?;
            write_json(&self.directory.join(format!("image-{index:02}.json")), &self.images[index], &mut self.metadata)?;
        }
        Ok(())
    }
}

fn write_image(directory: &std::path::Path, index: usize, image: &ColorImage) -> Result<(), String> {
    let file = OpenOptions::new().write(true).create_new(true).open(directory.join(format!("image-{index:02}.p7"))).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(file);
    write!(writer, "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", image.size[0], image.size[1])
        .map_err(|e| e.to_string())?;
    for pixel in &image.pixels {
        writer.write_all(&pixel.to_array()).map_err(|e| e.to_string())?;
    }
    writer.flush().map_err(|e| e.to_string())?;
    writer.get_ref().sync_all().map_err(|e| e.to_string())
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
            let reference = Arc::new(ColorImage::filled(reference_size, egui::Color32::WHITE));
            trace.image(0, ViewportId::ROOT, &token, &reference, &app, &ctx).unwrap();
            trace.draw(1, &app, &ctx).unwrap();
            let token = trace.request(1, &app, &ctx).unwrap();
            let gpu = Arc::new(ColorImage::filled(gpu_size, egui::Color32::WHITE));
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

    #[test]
    fn deferred_capture_retains_shared_pixels_without_files_until_finish_and_releases_them() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().join("new");
        let mut trace = Trace::deferred(directory.clone()).unwrap();
        let ctx = Context::default();
        let app = ViamindApp::with_scene_size(1);
        let originals: Vec<_> =
            (0..10).map(|n| Arc::new(ColorImage::filled([2, 1], egui::Color32::from_rgba_premultiplied(n, 20, 30, 255)))).collect();
        for (step, image) in originals.iter().enumerate() {
            trace.draw(step, &app, &ctx).unwrap();
            let token = trace.request(step, &app, &ctx).unwrap();
            trace.image(step, ViewportId::ROOT, &token, image, &app, &ctx).unwrap();
            assert_eq!(Arc::strong_count(image), 2, "retention shares the original pixel allocation");
            assert_eq!(fs::read_dir(&directory).unwrap().count(), 0, "no image or JSON I/O during sequence");
        }
        assert!(trace.image(10, ViewportId::ROOT, &UserData::default(), &originals[0], &app, &ctx).is_err());
        assert_eq!(trace.deferred_images.len(), 10);
        trace.finish(None).unwrap();
        for (index, original) in originals.iter().enumerate() {
            assert_eq!(Arc::strong_count(original), 1, "terminal flush releases diagnostic ownership");
            let raw = fs::read(directory.join(format!("image-{index:02}.p7"))).unwrap();
            let end = raw.windows(7).position(|w| w == b"ENDHDR\n").unwrap() + 7;
            let expected: Vec<_> = original.pixels.iter().flat_map(|p| p.to_array()).collect();
            assert_eq!(&raw[end..], expected);
            assert!(directory.join(format!("image-{index:02}.json")).is_file());
        }
        assert!(directory.join("trace.json").is_file());
    }

    #[test]
    fn deferred_abort_flushes_prior_pixels_and_raw_budget_refuses_before_retention() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().join("new");
        let mut trace = Trace::deferred(directory.clone()).unwrap();
        let ctx = Context::default();
        let app = ViamindApp::with_scene_size(1);
        let image = Arc::new(ColorImage::filled([2, 1], egui::Color32::WHITE));
        trace.draw(0, &app, &ctx).unwrap();
        let token = trace.request(0, &app, &ctx).unwrap();
        trace.image(0, ViewportId::ROOT, &token, &image, &app, &ctx).unwrap();
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        assert!(trace.finish(Some("explicit bounded abort")).is_err());
        assert_eq!(Arc::strong_count(&image), 1);
        assert!(directory.join("image-00.p7").is_file());
        let report = fs::read_to_string(directory.join("trace.json")).unwrap();
        assert!(report.contains("explicit bounded abort"));
        assert!(report.contains("deferred_io_legacy_transition_trace"));

        let mut capped = Trace::deferred(temporary.path().join("capped")).unwrap();
        capped.raw_bytes = MAX_RAW_BYTES - 7;
        capped.draw(0, &app, &ctx).unwrap();
        let token = capped.request(0, &app, &ctx).unwrap();
        assert!(capped.image(0, ViewportId::ROOT, &token, &image, &app, &ctx).is_err());
        assert_eq!(Arc::strong_count(&image), 1);
        assert!(capped.deferred_images.is_empty());
        assert!(capped.images.is_empty());
        assert_eq!(capped.raw_bytes, MAX_RAW_BYTES - 7);
    }

    #[test]
    fn navigation_observations_refuse_excess_events_and_never_retain_text() {
        let ctx = Context::default();
        let _ = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    Event::Text("private input text".into()),
                    Event::Paste("private clipboard".into()),
                    Event::PointerMoved(egui::pos2(10.0, 20.0)),
                ],
                ..Default::default()
            },
            |ui| {
                let input = navigation_input(ui.ctx()).unwrap();
                assert_eq!(input.pointer_motion, 1);
                let serialized = serde_json::to_string(&input).unwrap();
                assert!(!serialized.contains("private"));
                assert!(!serialized.contains("clipboard"));
            },
        );
        let _ = ctx
            .run_ui(egui::RawInput { events: vec![Event::PointerMoved(egui::Pos2::ZERO); MAX_INPUT_EVENTS + 1], ..Default::default() }, |ui| {
                assert!(navigation_input(ui.ctx()).unwrap_err().contains("input event count"))
            });
    }

    #[test]
    fn progress_state_and_full_output_are_charged_before_worker_creation() {
        let temporary = tempfile::tempdir().unwrap();
        let mut trace = Trace::deferred(temporary.path().join("deferred")).unwrap();
        trace.metadata.used = MAX_METADATA - parity_progress::BUDGET_BYTES + 1;
        assert!(trace.enable_progress().is_err());
        assert!(trace.progress.is_none());
        assert_eq!(fs::read_dir(&trace.directory).unwrap().count(), 0);
        let mut immediate = Trace::new(temporary.path().join("immediate")).unwrap();
        let prior = immediate.metadata.used;
        assert!(immediate.enable_progress().is_err());
        assert_eq!(immediate.metadata.used, prior);
    }

    #[test]
    fn hardened_admission_refuses_camera_draw_pair_identity_and_size_mismatches() {
        let ctx = Context::default();
        for failure in ["request_camera", "intervening_draw", "pair_camera", "foreign_viewport", "repeated_token", "larger", "smaller"] {
            let temporary = tempfile::tempdir().unwrap();
            let mut trace = Trace::deferred(temporary.path().join("strict")).unwrap();
            trace.enable_hardened().unwrap();
            let mut app = ViamindApp::with_scene_size(1);
            let first_size = if failure == "smaller" { [2, 2] } else { [1, 1] };
            let image = Arc::new(ColorImage::filled(first_size, egui::Color32::WHITE));
            trace.draw(0, &app, &ctx).unwrap();
            let first_token = trace.request(0, &app, &ctx).unwrap();
            if failure == "request_camera" {
                app.canvas.camera.center.x += 1.0;
                assert!(trace.image(0, ViewportId::ROOT, &first_token, &image, &app, &ctx).is_err());
            } else if failure == "intervening_draw" {
                let camera = app.canvas.camera;
                app.canvas.camera.center.x += 1.0;
                trace.draw(0, &app, &ctx).unwrap();
                app.canvas.camera = camera;
                assert!(trace.image(0, ViewportId::ROOT, &first_token, &image, &app, &ctx).is_err());
            } else if failure == "foreign_viewport" {
                assert!(trace.image(0, ViewportId::from_hash_of("foreign"), &first_token, &image, &app, &ctx).is_err());
            } else {
                trace.image(0, ViewportId::ROOT, &first_token, &image, &app, &ctx).unwrap();
                if failure == "pair_camera" {
                    app.canvas.camera.center.y += 1.0;
                }
                trace.draw(1, &app, &ctx).unwrap();
                let token = trace.request(1, &app, &ctx).unwrap();
                let token = if failure == "repeated_token" { &first_token } else { &token };
                let second_size = if failure == "larger" { [2, 2] } else { [1, 1] };
                let second = Arc::new(ColorImage::filled(second_size, egui::Color32::WHITE));
                assert!(trace.image(1, ViewportId::ROOT, token, &second, &app, &ctx).is_err(), "{failure}");
            }
            assert!(trace.finish(Some(failure)).is_err());
            assert!(trace.directory.join("image-00.p7").is_file());
            assert!(fs::read_to_string(trace.directory.join("trace.json")).unwrap().contains("hardened_manual_camera_completed_frame_trace"));
        }
    }

    #[test]
    fn hardened_navigation_refusal_accepts_only_harmless_pointer_motion_and_keeps_text_private() {
        let temporary = tempfile::tempdir().unwrap();
        let mut trace = Trace::deferred(temporary.path().join("strict")).unwrap();
        trace.enable_hardened().unwrap();
        for events in [
            vec![Event::Text("private text".into()), Event::PointerMoved(egui::pos2(5.0, 10.0))],
            vec![Event::Zoom(1.25)],
            vec![Event::Key { key: egui::Key::Home, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }],
            vec![Event::PointerButton {
                pos: egui::pos2(5.0, 10.0),
                button: egui::PointerButton::Middle,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            let should_pass = matches!(events.first(), Some(Event::Text(_)));
            let ctx = Context::default();
            let _ = ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| {
                assert_eq!(trace.check_hardened_input(ui.ctx()).is_ok(), should_pass);
                assert!(!serde_json::to_string(&navigation_input(ui.ctx()).unwrap()).unwrap().contains("private"));
            });
        }
        let mut capped = Trace::deferred(temporary.path().join("capped")).unwrap();
        capped.metadata.used = MAX_METADATA;
        assert!(capped.enable_hardened().is_err());
        assert!(!capped.hardened && capped.settling.is_none());
    }
}
