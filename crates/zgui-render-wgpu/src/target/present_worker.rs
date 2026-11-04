//! A bounded FIFO presentation queue. Only this thread touches the swapchain after startup.
use crate::{
    gpu::{device::Gpu, formats::Formats, surface::ConfiguredSurface},
    target::acquire::Acquisition,
};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use zgui_geom::{Device, Size};
use zgui_render::RenderTarget;

/// Platform hooks for off-thread FIFO presentation.
pub struct PresentHooks {
    /// Waits outside graphics-API locks until the previous compositor frame is ready.
    pub ready: Box<dyn Fn(&AtomicBool) -> bool + Send + Sync>,
    /// Applies metadata for this exact buffer; false discards a stale or hidden frame.
    pub prepare: Box<dyn Fn(RenderTarget) -> bool + Send + Sync>,
    /// Ends the platform commit transaction, including after a failed presentation.
    pub complete: Box<dyn Fn() + Send + Sync>,
    /// Requests a redraw of this window when the composition slot becomes free.
    pub wake: Box<dyn Fn() + Send + Sync>,
}

pub(crate) struct Frame {
    pub target: RenderTarget,
    pub binding: wgpu::BindGroup,
    pub pipeline: wgpu::RenderPipeline,
    pub generation: u64,
}

#[derive(Default)]
struct Slot<T> {
    reserved: bool,
    pending: Option<T>,
}
impl<T> Slot<T> {
    fn reserve(&mut self) -> bool {
        if self.reserved {
            return false;
        }
        self.reserved = true;
        true
    }
    fn release(&mut self) {
        self.pending = None;
        self.reserved = false;
    }
}
struct State {
    slot: Slot<Frame>,
    generation: u64,
    size: Size<i32, Device>,
    failed: bool,
    reconfigure: bool,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    stopped: AtomicBool,
    retry: AtomicBool,
    hooks: PresentHooks,
}

/// UI-side owner of one worker and one pending composition slot.
pub struct PresentWorker {
    shared: Arc<Shared>,
    formats: Formats,
    mode: wgpu::PresentMode,
}
impl std::fmt::Debug for PresentWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PresentWorker")
            .field("size", &self.size())
            .finish()
    }
}
impl PresentWorker {
    pub(crate) fn new(gpu: Arc<Gpu>, surface: Box<ConfiguredSurface>, hooks: PresentHooks) -> Self {
        let formats = surface.formats();
        let mode = surface.present_mode();
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                slot: Slot {
                    reserved: false,
                    pending: None,
                },
                generation: 0,
                size: surface.size(),
                failed: false,
                reconfigure: false,
            }),
            changed: Condvar::new(),
            stopped: AtomicBool::new(false),
            retry: AtomicBool::new(false),
            hooks,
        });
        let thread_shared = shared.clone();
        std::thread::Builder::new()
            .name("zgui-present".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(gpu, surface, &thread_shared)
                }));
                if result.is_err() {
                    thread_shared
                        .state
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .failed = true;
                    (thread_shared.hooks.wake)();
                    tracing::error!("presentation worker failed");
                }
            })
            .expect("start presentation worker");
        Self {
            shared,
            formats,
            mode,
        }
    }
    pub(crate) fn take_retry(&self) -> bool {
        self.shared.retry.swap(false, Ordering::AcqRel)
    }
    pub(crate) fn formats(&self) -> Formats {
        self.formats
    }
    pub(crate) fn size(&self) -> Size<i32, Device> {
        self.shared.state.lock().unwrap().size
    }
    pub(crate) fn mode(&self) -> wgpu::PresentMode {
        self.mode
    }
    pub(crate) fn request_reconfigure(&self) {
        self.shared.state.lock().unwrap().reconfigure = true;
    }
    pub(crate) fn failed(&self) -> bool {
        self.shared.state.lock().unwrap().failed
    }
    pub(crate) fn resize(&self, size: Size<i32, Device>) {
        let mut state = self.shared.state.lock().unwrap();
        if state.size != size {
            state.size = size;
            state.generation += 1;
        }
        self.shared.changed.notify_all();
    }
    pub(crate) fn reserve(&self) -> Option<Permit> {
        let mut state = self.shared.state.lock().unwrap();
        if state.failed || !state.slot.reserve() {
            return None;
        }
        Some(Permit {
            shared: self.shared.clone(),
            generation: state.generation,
            submitted: false,
        })
    }
}
impl Drop for PresentWorker {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.shared.changed.notify_all();
        // Never join a thread that may be inside a display driver. Its owned handles keep the
        // surface/device alive until the in-flight call returns; cancellation ends its next wait.
    }
}

pub(crate) struct Permit {
    shared: Arc<Shared>,
    pub generation: u64,
    submitted: bool,
}
impl Permit {
    pub(crate) fn submit(mut self, frame: Frame) {
        self.shared.state.lock().unwrap().slot.pending = Some(frame);
        self.submitted = true;
        self.shared.changed.notify_one();
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        if !self.submitted {
            self.shared.state.lock().unwrap().slot.release();
            self.shared.changed.notify_one();
        }
    }
}

fn release(shared: &Shared) {
    shared.state.lock().unwrap().slot.release();
    (shared.hooks.wake)();
}

fn run(gpu: Arc<Gpu>, mut surface: Box<ConfiguredSurface>, shared: &Shared) {
    loop {
        let frame = {
            let mut state = shared.state.lock().unwrap();
            while state.slot.pending.is_none() && !shared.stopped.load(Ordering::Acquire) {
                state = shared.changed.wait(state).unwrap();
            }
            if shared.stopped.load(Ordering::Acquire) {
                return;
            }
            state.slot.pending.take().unwrap()
        };
        if !(shared.hooks.ready)(&shared.stopped) {
            surface.request_reconfigure();
            shared.retry.store(true, Ordering::Release);
            release(shared);
            continue;
        }
        if shared.stopped.load(Ordering::Acquire) {
            return;
        }
        if frame.generation != shared.state.lock().unwrap().generation {
            shared.retry.store(true, Ordering::Release);
            release(shared);
            continue;
        }
        if std::mem::take(&mut shared.state.lock().unwrap().reconfigure) {
            surface.request_reconfigure();
        }
        surface.resize(frame.target.size);
        surface.apply(&gpu);
        let acquired = surface.surface().get_current_texture();
        let answer = Acquisition::classify(&acquired);
        let texture = match acquired {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            _ => {
                surface.request_reconfigure();
                shared.retry.store(true, Ordering::Release);
                let fatal = matches!(answer, Acquisition::Validation | Acquisition::Lost);
                if fatal {
                    shared.state.lock().unwrap().failed = true;
                }
                release(shared);
                if fatal {
                    return;
                }
                // A failed acquisition must not become a hot retry loop on a hidden surface.
                let state = shared.state.lock().unwrap();
                let _ = shared
                    .changed
                    .wait_timeout_while(state, Duration::from_millis(100), |_| {
                        !shared.stopped.load(Ordering::Acquire)
                    })
                    .unwrap();
                continue;
            }
        };
        if frame.generation != shared.state.lock().unwrap().generation
            || shared.stopped.load(Ordering::Acquire)
        {
            drop(texture);
            shared.retry.store(true, Ordering::Release);
            release(shared);
            continue;
        }
        let view = surface.present_view(&texture.texture);
        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("zgui.present-worker"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("zgui.present-worker"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&frame.pipeline);
            pass.set_bind_group(0, &frame.binding, &[]);
            pass.draw(0..4, 0..1);
        }
        gpu.queue().submit([encoder.finish()]);
        // The shared composed texture can be written again only after its blit was queued on the
        // same GPU queue. Otherwise the worker could present pixels from a newer scene.
        release(shared);
        if !(shared.hooks.prepare)(frame.target) {
            drop(texture);
            shared.retry.store(true, Ordering::Release);
            (shared.hooks.wake)();
            continue;
        }
        struct Complete<'a>(&'a PresentHooks);
        impl Drop for Complete<'_> {
            fn drop(&mut self) {
                (self.0.complete)();
            }
        }
        let complete = Complete(&shared.hooks);
        texture.present();
        drop(complete);
        if answer == Acquisition::Suboptimal {
            surface.request_reconfigure();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Slot;
    fn idle_worker() -> super::PresentWorker {
        use crate::gpu::formats::{Formats, SrgbTier};
        use std::sync::{Arc, Condvar, Mutex, atomic::AtomicBool};
        super::PresentWorker {
            shared: Arc::new(super::Shared {
                state: Mutex::new(super::State {
                    slot: Slot {
                        reserved: false,
                        pending: None,
                    },
                    generation: 0,
                    size: zgui_geom::Size::new(800, 600),
                    failed: false,
                    reconfigure: false,
                }),
                changed: Condvar::new(),
                stopped: AtomicBool::new(false),
                retry: AtomicBool::new(false),
                hooks: super::PresentHooks {
                    ready: Box::new(|_| true),
                    prepare: Box::new(|_| true),
                    complete: Box::new(|| {}),
                    wake: Box::new(|| {}),
                },
            }),
            formats: Formats {
                surface: wgpu::TextureFormat::Bgra8Unorm,
                scene: wgpu::TextureFormat::Bgra8Unorm,
                scratch: wgpu::TextureFormat::Rgba8Unorm,
                view_format_twin: None,
                tier: SrgbTier::Native,
                alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            },
            mode: wgpu::PresentMode::Fifo,
        }
    }

    #[test]
    fn an_undamaged_or_failed_recording_releases_its_reservation() {
        let worker = idle_worker();
        let permit = worker.reserve().unwrap();
        assert!(worker.reserve().is_none());
        drop(permit);
        assert!(worker.reserve().is_some());
    }

    #[test]
    fn a_resize_invalidates_an_older_frame_without_waiting_for_presentation() {
        let worker = idle_worker();
        let old = worker.reserve().unwrap();
        worker.resize(zgui_geom::Size::new(1200, 750));
        assert_ne!(
            old.generation,
            worker.shared.state.lock().unwrap().generation
        );
        drop(old);
        let fresh = worker.reserve().unwrap();
        assert_eq!(
            fresh.generation,
            worker.shared.state.lock().unwrap().generation
        );
        assert_eq!(worker.size(), zgui_geom::Size::new(1200, 750));
    }

    #[test]
    fn worker_failure_stops_accepting_compositions() {
        let worker = idle_worker();
        worker.shared.state.lock().unwrap().failed = true;
        assert!(worker.failed());
        assert!(worker.reserve().is_none());
    }

    #[test]
    fn shutdown_cancels_readiness_without_joining_a_display_wait() {
        let worker = idle_worker();
        let shared = worker.shared.clone();
        drop(worker);
        assert!(shared.stopped.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn one_composition_waits_while_the_previous_frame_is_presented() {
        let mut slot = Slot::<u32> {
            reserved: false,
            pending: None,
        };
        assert!(slot.reserve());
        slot.pending = Some(1);
        for _ in 0..1000 {
            assert!(!slot.reserve());
        }
        assert_eq!(slot.pending.take(), Some(1));
        assert!(
            !slot.reserve(),
            "taking the frame cannot allow its source texture to change"
        );
        slot.release();
        assert!(
            slot.reserve(),
            "the blit submission releases the next composition"
        );
    }
}
