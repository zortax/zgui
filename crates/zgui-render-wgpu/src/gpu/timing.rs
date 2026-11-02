//! Optional GPU pass timing through timestamp queries.
//!
//! Off by default. Timing is on when [`ENV`] is set (or [`enable`] was called) before the device
//! opens, and the adapter offers [`wgpu::Features::TIMESTAMP_QUERY`]. Only then does the device ask
//! for the feature and does a renderer create query sets. When timing is off, a frame records
//! exactly the passes it records without this module.
//!
//! Each timed frame writes a begin and an end timestamp per pass into a query set of its own. The
//! frame resolves the queries into a buffer in its last submission and maps a copy of them
//! asynchronously. A later frame reads the copy when the map has completed, so a result arrives one
//! or two frames after the frame it describes, and no frame waits for one. A frame that finds every
//! query set still in flight goes untimed.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};

use zgui_profile::{Counter, counter};

use crate::gpu::device::Gpu;

/// The environment variable that turns pass timing on.
///
/// Any value turns it on, except an empty value and `0`.
pub const ENV: &str = "ZGUI_GPU_TIMING";

/// How many queries one frame can write: a begin and an end for each of 256 passes.
const QUERIES: u32 = 512;

/// How many frames can have results in flight at once.
const SLOTS: usize = 3;

/// How many completed results are kept for [`GpuTimer::take`].
const KEPT: usize = 1024;

/// Set by [`enable`].
static ENABLED: AtomicBool = AtomicBool::new(false);

/// Turns pass timing on for each device opened after this call, as [`ENV`] does.
pub fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

/// Whether pass timing was asked for, through [`ENV`] or [`enable`].
pub fn requested() -> bool {
    static FROM_ENV: OnceLock<bool> = OnceLock::new();
    ENABLED.load(Ordering::Relaxed)
        || *FROM_ENV.get_or_init(|| {
            std::env::var_os(ENV).is_some_and(|value| !value.is_empty() && value != "0")
        })
}

/// The kind of GPU work a timed pass does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimedWork {
    /// Draws into the composed target or into a bin page: instanced batches and composites.
    Draws,
    /// Draws into an isolated target: group content and filter steps.
    Groups,
    /// The vector rasteriser's work for one round.
    Vectors,
    /// The copy of the composed target onto the presented texture.
    Blit,
}

/// How long the GPU spent on one frame's timed passes.
///
/// Copies outside passes are not timed: the scroll shift copy and region copies between targets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GpuTiming {
    /// Which frame this is, counted from zero over the frames the renderer recorded.
    pub frame: u64,
    /// Milliseconds in [`TimedWork::Draws`] passes.
    pub draws_ms: f64,
    /// Milliseconds in [`TimedWork::Groups`] passes.
    pub groups_ms: f64,
    /// Milliseconds in [`TimedWork::Vectors`] spans.
    ///
    /// Each span starts at a marker submitted before the rasteriser clears its targets and stops
    /// at a marker submitted after its last submission. It also holds the time the GPU waits while
    /// the rasteriser encodes, so it is an upper bound on the rasteriser's GPU time.
    pub vectors_ms: f64,
    /// Milliseconds in the [`TimedWork::Blit`] pass.
    pub blit_ms: f64,
    /// Passes that got no queries because the frame had used all of them.
    pub untimed: u32,
}

impl GpuTiming {
    /// The sum of every kind, in milliseconds.
    pub fn total_ms(&self) -> f64 {
        self.draws_ms + self.groups_ms + self.vectors_ms + self.blit_ms
    }

    /// Adds `ticks` of `work` at `period_ns` nanoseconds per tick.
    fn add(&mut self, work: TimedWork, ticks: u64, period_ns: f64) {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a pass shorter than 2^52 ticks is exact"
        )]
        let ms = ticks as f64 * period_ns * 1e-6;
        match work {
            TimedWork::Draws => self.draws_ms += ms,
            TimedWork::Groups => self.groups_ms += ms,
            TimedWork::Vectors => self.vectors_ms += ms,
            TimedWork::Blit => self.blit_ms += ms,
        }
    }
}

/// The two queries of one timed pass.
pub(crate) struct PassQueries {
    /// The frame's query set.
    set: wgpu::QuerySet,
    /// The begin query. The end query follows it.
    first: u32,
}

impl PassQueries {
    /// The timestamp writes of a render pass timed by these queries.
    pub(crate) fn render(&self) -> wgpu::RenderPassTimestampWrites<'_> {
        wgpu::RenderPassTimestampWrites {
            query_set: &self.set,
            beginning_of_pass_write_index: Some(self.first),
            end_of_pass_write_index: Some(self.first + 1),
        }
    }
}

/// What a query slot is doing.
enum State {
    /// Ready for a frame.
    Free,
    /// A frame records into it.
    Recording,
    /// Its readback buffer is being mapped. The flag is 0 while pending, 1 when mapped, 2 when
    /// the map failed.
    Mapping(Arc<AtomicU8>),
}

/// The queries and buffers of one frame in flight.
struct Slot {
    /// The query set.
    queries: wgpu::QuerySet,
    /// Where the queries resolve to.
    resolve: wgpu::Buffer,
    /// The mappable copy of `resolve`.
    readback: wgpu::Buffer,
    /// What the slot is doing.
    state: State,
    /// How many queries the frame wrote.
    used: u32,
    /// The kind of each query pair, in order.
    work: Vec<TimedWork>,
    /// Which frame records into the slot.
    frame: u64,
    /// Passes that got no queries.
    untimed: u32,
}

impl Slot {
    /// Creates one slot on `gpu`.
    fn new(gpu: &Gpu) -> Self {
        let device = gpu.device();
        let bytes = u64::from(QUERIES) * u64::from(wgpu::QUERY_SIZE);
        Self {
            queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("zgui.timing.queries"),
                ty: wgpu::QueryType::Timestamp,
                count: QUERIES,
            }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("zgui.timing.resolve"),
                size: bytes,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("zgui.timing.readback"),
                size: bytes,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            state: State::Free,
            used: 0,
            work: Vec::new(),
            frame: 0,
            untimed: 0,
        }
    }

    /// The bytes the frame's queries resolve to.
    fn bytes(&self) -> u64 {
        u64::from(self.used) * u64::from(wgpu::QUERY_SIZE)
    }
}

/// The per-renderer half of pass timing.
pub(crate) struct GpuTimer {
    /// One slot per frame in flight.
    slots: Vec<Slot>,
    /// The slot the current frame records into, if it got one.
    recording: Option<usize>,
    /// Nanoseconds per timestamp tick.
    period_ns: f64,
    /// How many frames began.
    frames: u64,
    /// Completed results, oldest first.
    done: VecDeque<GpuTiming>,
    /// The most recent completed result.
    latest: Option<GpuTiming>,
}

impl GpuTimer {
    /// A timer on `gpu`, when timing was asked for and the device has timestamp queries.
    pub(crate) fn new(gpu: &Gpu) -> Option<Self> {
        if !requested()
            || !gpu
                .device()
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            return None;
        }
        Some(Self {
            slots: (0..SLOTS).map(|_| Slot::new(gpu)).collect(),
            recording: None,
            period_ns: f64::from(gpu.queue().get_timestamp_period()),
            frames: 0,
            done: VecDeque::new(),
            latest: None,
        })
    }

    /// How many frames began recording, timed or not.
    pub(crate) fn frames(&self) -> u64 {
        self.frames
    }

    /// The most recent completed result.
    pub(crate) fn latest(&self) -> Option<GpuTiming> {
        self.latest
    }

    /// Starts a frame: reads completed results and gives the frame a free slot, if one is free.
    pub(crate) fn begin(&mut self, gpu: &Gpu) {
        self.collect(gpu);
        let frame = self.frames;
        self.frames += 1;
        self.recording = self
            .slots
            .iter()
            .position(|slot| matches!(slot.state, State::Free));
        if let Some(index) = self.recording {
            let slot = &mut self.slots[index];
            slot.state = State::Recording;
            slot.used = 0;
            slot.work.clear();
            slot.frame = frame;
            slot.untimed = 0;
        }
    }

    /// Gives one pass of `work` its two queries, or `None` for an untimed frame or a full slot.
    pub(crate) fn pass(&mut self, work: TimedWork) -> Option<PassQueries> {
        let slot = &mut self.slots[self.recording?];
        if slot.used + 2 > QUERIES {
            slot.untimed += 1;
            return None;
        }
        let first = slot.used;
        slot.used += 2;
        slot.work.push(work);
        Some(PassQueries {
            set: slot.queries.clone(),
            first,
        })
    }

    /// Submits the marker that opens a [`TimedWork::Vectors`] span.
    pub(crate) fn open_vectors(&mut self, gpu: &Gpu) -> Option<PassQueries> {
        let queries = self.pass(TimedWork::Vectors)?;
        marker(gpu, &queries.set, None, Some(queries.first));
        Some(queries)
    }

    /// Submits the marker that closes the span `queries` opened.
    pub(crate) fn close_vectors(gpu: &Gpu, queries: &PassQueries) {
        marker(gpu, &queries.set, Some(queries.first + 1), None);
    }

    /// Resolves the frame's queries into its readback buffer, as the frame's last commands.
    pub(crate) fn finish(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let Some(index) = self.recording else {
            return;
        };
        let slot = &mut self.slots[index];
        if slot.used == 0 {
            slot.state = State::Free;
            self.recording = None;
            return;
        }
        encoder.resolve_query_set(&slot.queries, 0..slot.used, &slot.resolve, 0);
        encoder.copy_buffer_to_buffer(&slot.resolve, 0, &slot.readback, 0, slot.bytes());
    }

    /// Starts the map of the frame's readback buffer, after the frame's last submission.
    pub(crate) fn submitted(&mut self) {
        let Some(index) = self.recording.take() else {
            return;
        };
        let slot = &mut self.slots[index];
        let flag = Arc::new(AtomicU8::new(0));
        let mapped = Arc::clone(&flag);
        slot.readback
            .slice(0..slot.bytes())
            .map_async(wgpu::MapMode::Read, move |result| {
                mapped.store(if result.is_ok() { 1 } else { 2 }, Ordering::Release);
            });
        slot.state = State::Mapping(flag);
    }

    /// Reads every completed map without waiting for one.
    pub(crate) fn collect(&mut self, gpu: &Gpu) {
        let pending = |slot: &Slot| match &slot.state {
            State::Mapping(flag) => flag.load(Ordering::Acquire) == 0,
            State::Free | State::Recording => false,
        };
        if self.slots.iter().any(pending) {
            let _ = gpu.device().poll(wgpu::PollType::Poll);
        }
        let mut completed = Vec::new();
        for slot in &mut self.slots {
            let State::Mapping(flag) = &slot.state else {
                continue;
            };
            match flag.load(Ordering::Acquire) {
                0 => continue,
                1 => {
                    completed.push(read(slot, self.period_ns));
                    slot.readback.unmap();
                }
                _ => {}
            }
            slot.state = State::Free;
        }
        completed.sort_by_key(|timing| timing.frame);
        for timing in completed {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a frame's GPU time in nanoseconds is far below 2^64"
            )]
            counter::add(Counter::GpuTimeNs, (timing.total_ms() * 1e6) as u64);
            if self.done.len() == KEPT {
                self.done.pop_front();
            }
            self.done.push_back(timing);
            self.latest = Some(timing);
        }
    }

    /// Reads completed maps and hands over every result held, oldest first.
    pub(crate) fn take(&mut self, gpu: &Gpu) -> Vec<GpuTiming> {
        self.collect(gpu);
        self.done.drain(..).collect()
    }
}

/// Reads one mapped slot.
fn read(slot: &Slot, period_ns: f64) -> GpuTiming {
    let mut timing = GpuTiming {
        frame: slot.frame,
        untimed: slot.untimed,
        ..GpuTiming::default()
    };
    let view = slot.readback.slice(0..slot.bytes()).get_mapped_range();
    let mut stamps = view.chunks_exact(8).map(|bytes| {
        let mut word = [0; 8];
        word.copy_from_slice(bytes);
        u64::from_le_bytes(word)
    });
    for &work in &slot.work {
        let (Some(begin), Some(end)) = (stamps.next(), stamps.next()) else {
            break;
        };
        timing.add(work, end.saturating_sub(begin), period_ns);
    }
    timing
}

/// Submits an empty compute pass that writes the given timestamps.
fn marker(gpu: &Gpu, set: &wgpu::QuerySet, begin: Option<u32>, end: Option<u32>) {
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("zgui.timing.marker"),
        });
    drop(encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("zgui.timing.marker"),
        timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
            query_set: set,
            beginning_of_pass_write_index: begin,
            end_of_pass_write_index: end,
        }),
    }));
    gpu.queue().submit([encoder.finish()]);
}
