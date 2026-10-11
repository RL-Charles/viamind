//! Opt-in timing of production board GPU commands. No queries or waits occur unless attached.
use eframe::wgpu;
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

const SLOTS: usize = 4;
const HISTORY: usize = 4096;
/// Identity supplied by a diagnostic caller before building a native frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FrameTag {
    pub frame: u64,
    pub phase: usize,
    pub sampled: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct GpuSample {
    pub tag: FrameTag,
    pub gpu_ms: f64,
}
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Counters {
    pub skipped_busy: usize,
    pub discarded_callbacks: usize,
    pub readback_errors: usize,
    pub history_overflow: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Selected,
    Recorded,
    Mapping,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Token {
    index: usize,
    serial: u64,
    tag: FrameTag,
}
#[derive(Default)]
struct State {
    slots: [Option<(Token, Stage)>; SLOTS],
    serial: u64,
    tag: FrameTag,
    samples: VecDeque<GpuSample>,
    counters: Counters,
    errors: VecDeque<String>,
}
impl State {
    fn reserve(&mut self, tag: FrameTag) -> Option<Token> {
        let Some(index) = self.slots.iter().position(Option::is_none) else {
            self.counters.skipped_busy += 1;
            return None;
        };
        self.serial = self.serial.wrapping_add(1);
        let token = Token { index, serial: self.serial, tag };
        self.slots[index] = Some((token, Stage::Selected));
        Some(token)
    }
    fn transition(&mut self, token: Token, from: Stage, to: Stage) -> bool {
        if self.slots[token.index] == Some((token, from)) {
            self.slots[token.index] = Some((token, to));
            true
        } else {
            false
        }
    }
    fn discard(&mut self, token: Token) {
        if self.slots[token.index] == Some((token, Stage::Selected)) {
            self.slots[token.index] = None;
            self.counters.discarded_callbacks += 1;
        }
    }
    fn complete(&mut self, token: Token, value: Result<f64, String>) {
        if self.slots[token.index] != Some((token, Stage::Mapping)) {
            return;
        }
        self.slots[token.index] = None;
        match value {
            Ok(gpu_ms) => {
                if self.samples.len() == HISTORY {
                    self.samples.pop_front();
                    self.counters.history_overflow += 1;
                }
                self.samples.push_back(GpuSample { tag: token.tag, gpu_ms });
            }
            Err(error) => {
                self.counters.readback_errors += 1;
                if self.errors.len() == 16 {
                    self.errors.pop_front();
                }
                self.errors.push_back(error);
            }
        }
    }
}
struct Slot {
    query: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
}
/// Optional diagnostic handle. Normal renderer setup does not request its device features.
pub struct GpuTiming {
    device: wgpu::Device,
    queue: wgpu::Queue,
    slots: Vec<Slot>,
    state: Mutex<State>,
}
#[derive(Serialize)]
pub struct Report {
    pub samples: Vec<GpuSample>,
    pub counters: Counters,
    pub errors: Vec<String>,
    pub pending_slots: usize,
    pub resource_slots: usize,
    pub history_capacity: usize,
    pub timestamp_period_ns: f64,
    pub enabled_features: String,
}
impl GpuTiming {
    pub fn required_features() -> wgpu::Features {
        wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES
    }
    /// Returns unavailable when the diagnostic features were not enabled on this device.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Option<Arc<Self>> {
        if !device.features().contains(Self::required_features()) {
            return None;
        }
        let slots = (0..SLOTS)
            .map(|_| Slot {
                query: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("board diagnostic timestamps"),
                    ty: wgpu::QueryType::Timestamp,
                    count: 2,
                }),
                resolve: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("board timestamp resolve"),
                    size: 256,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                read: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("board timestamp readback"),
                    size: 16,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
            })
            .collect();
        Some(Arc::new(Self { device, queue, slots, state: Mutex::new(State::default()) }))
    }
    pub fn set_frame(&self, tag: FrameTag) {
        self.state.lock().unwrap().tag = tag;
    }
    pub(crate) fn frame_tag(&self) -> FrameTag {
        self.state.lock().unwrap().tag
    }
    pub fn report(&self) -> Report {
        let state = self.state.lock().unwrap();
        let mut samples: Vec<_> = state.samples.iter().cloned().collect();
        samples.sort_by_key(|s| s.tag.frame);
        Report {
            samples,
            counters: state.counters,
            errors: state.errors.iter().cloned().collect(),
            pending_slots: state.slots.iter().flatten().count(),
            resource_slots: SLOTS,
            history_capacity: HISTORY,
            timestamp_period_ns: f64::from(self.queue.get_timestamp_period()),
            enabled_features: format!("{:?}", self.device.features()),
        }
    }
    /// A bounded wait after cadence collection has ended; never called by the production UI.
    pub fn finish(self: &Arc<Self>) -> Result<(), String> {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("final board timestamp drain") });
        self.resolve(&mut encoder);
        let submission = self.queue.submit([encoder.finish()]);
        self.device
            .poll(wgpu::PollType::Wait { submission_index: Some(submission), timeout: Some(Duration::from_secs(10)) })
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub(crate) fn begin(self: &Arc<Self>, encoder: &mut wgpu::CommandEncoder, tag: FrameTag) -> Option<DrawTiming> {
        self.resolve(encoder);
        let token = self.state.lock().unwrap().reserve(tag)?;
        Some(DrawTiming { probe: Arc::clone(self), token })
    }
    fn resolve(self: &Arc<Self>, encoder: &mut wgpu::CommandEncoder) {
        let tokens: Vec<_> = {
            let mut state = self.state.lock().unwrap();
            let tokens: Vec<_> = state.slots.iter().flatten().filter_map(|(t, s)| (*s == Stage::Recorded).then_some(*t)).collect();
            for &token in &tokens {
                state.transition(token, Stage::Recorded, Stage::Mapping);
            }
            tokens
        };
        for token in tokens {
            let slot = &self.slots[token.index];
            encoder.resolve_query_set(&slot.query, 0..2, &slot.resolve, 0);
            encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.read, 0, 16);
            let read = slot.read.clone();
            let weak = Arc::downgrade(self);
            let period = f64::from(self.queue.get_timestamp_period());
            encoder.map_buffer_on_submit(&slot.read, wgpu::MapMode::Read, .., move |result| {
                let value = result.map_err(|e| e.to_string()).and_then(|()| {
                    let mapped = read.slice(..).get_mapped_range().map_err(|e| e.to_string())?;
                    duration(&mapped, period)
                });
                read.unmap();
                if let Some(probe) = weak.upgrade() {
                    probe.state.lock().unwrap().complete(token, value);
                }
            });
        }
    }
}
fn duration(bytes: &[u8], period: f64) -> Result<f64, String> {
    let values = bytes.as_chunks::<8>().0;
    if values.len() != 2 || bytes.len() != 16 || !period.is_finite() || period <= 0.0 {
        return Err("invalid timestamp payload or period".into());
    }
    let begin = u64::from_le_bytes(values[0]);
    let end = u64::from_le_bytes(values[1]);
    let ticks = end.checked_sub(begin).ok_or("timestamp end precedes begin")?;
    Ok(ticks as f64 * period / 1_000_000.0)
}
pub(crate) struct DrawTiming {
    probe: Arc<GpuTiming>,
    token: Token,
}
impl DrawTiming {
    pub fn start(&self, pass: &mut wgpu::RenderPass<'static>) -> bool {
        if self.probe.state.lock().unwrap().slots[self.token.index] != Some((self.token, Stage::Selected)) {
            return false;
        }
        pass.write_timestamp(&self.probe.slots[self.token.index].query, 0);
        true
    }
    pub fn end(&self, pass: &mut wgpu::RenderPass<'static>) {
        pass.write_timestamp(&self.probe.slots[self.token.index].query, 1);
        self.probe.state.lock().unwrap().transition(self.token, Stage::Selected, Stage::Recorded);
    }
}
impl Drop for DrawTiming {
    fn drop(&mut self) {
        self.probe.state.lock().unwrap().discard(self.token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tag(frame: u64) -> FrameTag {
        FrameTag { frame, phase: frame as usize % 3, sampled: true }
    }
    fn mapping(state: &mut State, token: Token) {
        assert!(state.transition(token, Stage::Selected, Stage::Recorded));
        assert!(state.transition(token, Stage::Recorded, Stage::Mapping));
    }
    #[test]
    fn backpressure_and_discarded_callbacks_preserve_new_reservations() {
        let mut state = State::default();
        let tokens: Vec<_> = (0..SLOTS).map(|i| state.reserve(tag(i as u64)).unwrap()).collect();
        assert!(state.reserve(tag(99)).is_none());
        assert_eq!(state.counters.skipped_busy, 1);
        state.discard(tokens[0]);
        let replacement = state.reserve(tag(100)).unwrap();
        assert_eq!(replacement.index, tokens[0].index);
        assert_ne!(replacement.serial, tokens[0].serial);
        state.discard(tokens[0]);
        assert_eq!(state.slots[replacement.index], Some((replacement, Stage::Selected)));
        assert_eq!(state.counters.discarded_callbacks, 1);
        assert_eq!(replacement.tag, tag(100));
    }
    #[test]
    fn delayed_mapping_completion_recycles_only_its_own_generation() {
        let mut state = State::default();
        let tokens: Vec<_> = (0..SLOTS).map(|i| state.reserve(tag(i as u64)).unwrap()).collect();
        for &token in &tokens {
            mapping(&mut state, token);
            state.discard(token);
        }
        assert!(state.reserve(tag(99)).is_none(), "mapped buffers cannot be reused before completion");
        state.complete(tokens[2], Ok(1.25));
        let replacement = state.reserve(tag(100)).unwrap();
        assert_eq!(replacement.index, 2);
        state.complete(tokens[2], Ok(99.0));
        assert_eq!(state.slots[2], Some((replacement, Stage::Selected)), "late duplicate completion cannot free a reused slot");
        assert_eq!(state.samples.len(), 1);
        assert_eq!(state.samples[0].tag, tag(2));
        assert_eq!(state.samples[0].gpu_ms, 1.25);
    }
    #[test]
    fn history_and_error_storage_stay_bounded_and_failed_readback_releases_slot() {
        let mut state = State::default();
        for frame in 0..HISTORY + 10 {
            let token = state.reserve(tag(frame as u64)).unwrap();
            mapping(&mut state, token);
            state.complete(token, Ok(0.25));
        }
        assert_eq!(state.samples.len(), HISTORY);
        assert_eq!(state.samples.front().unwrap().tag.frame, 10);
        assert_eq!(state.counters.history_overflow, 10);
        for i in 0..20 {
            let token = state.reserve(tag(i)).unwrap();
            mapping(&mut state, token);
            state.complete(token, Err(format!("mapping failure {i}")));
        }
        assert_eq!(state.counters.readback_errors, 20);
        assert_eq!(state.errors.len(), 16);
        assert!(state.slots.iter().all(Option::is_none));
    }
    #[test]
    fn timestamp_payload_uses_queue_period_and_rejects_invalid_values() {
        let bytes: Vec<_> = [1_000u64, 1_000_001_000].into_iter().flat_map(u64::to_le_bytes).collect();
        assert_eq!(duration(&bytes, 2.0).unwrap(), 2000.0);
        assert!(duration(&bytes[..8], 1.0).is_err());
        assert!(duration(&bytes, f64::NAN).is_err());
        assert!(duration(&bytes, 0.0).is_err());
        let reversed: Vec<_> = [100u64, 99].into_iter().flat_map(u64::to_le_bytes).collect();
        assert!(duration(&reversed, 1.0).is_err());
    }
}
