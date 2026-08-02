//! A surface that is a buffer, with no window behind it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use accesskit::TreeUpdate;
use zgui_geom::{Css, CssPx, Device, DevicePx, Size};

use crate::surface::{CursorStyle, FullscreenMode, Surface, SurfaceId, TextInput};

/// A surface that is a buffer, with no window behind it.
///
/// Everything a real surface would do to a window is recorded instead, because a headless backend
/// exists to be asserted against: a test that cannot see whether a frame was asked for cannot
/// check the loop's parking behaviour, which is the hardest part of the frame loop to get right.
#[derive(Debug)]
pub(in crate::backends) struct OffscreenSurface {
    id: SurfaceId,
    size: Mutex<Size<DevicePx, Device>>,
    redraws: AtomicU64,
    a11y_updates: AtomicU64,
    visible: AtomicBool,
}

impl OffscreenSurface {
    /// A surface of `size`, hidden, with nothing recorded yet.
    pub(in crate::backends) fn new(id: SurfaceId, size: Size<DevicePx, Device>) -> Self {
        Self {
            id,
            size: Mutex::new(size),
            redraws: AtomicU64::new(0),
            a11y_updates: AtomicU64::new(0),
            visible: AtomicBool::new(false),
        }
    }

    /// How many frames have been asked for since this surface was created.
    pub(in crate::backends) fn redraws(&self) -> u64 {
        self.redraws.load(Ordering::Relaxed)
    }

    /// How many accessibility updates have been published.
    pub(in crate::backends) fn a11y_updates(&self) -> u64 {
        self.a11y_updates.load(Ordering::Relaxed)
    }

    /// Whether the surface has been shown.
    pub(in crate::backends) fn is_visible(&self) -> bool {
        self.visible.load(Ordering::Relaxed)
    }
}

impl Surface for OffscreenSurface {
    fn id(&self) -> SurfaceId {
        self.id
    }

    fn size(&self) -> Size<DevicePx, Device> {
        *self.size.lock().expect("the size is not poisoned")
    }

    fn scale_factor(&self) -> f64 {
        1.0
    }

    fn request_size(&self, size: Size<CssPx, Css>) -> Option<Size<DevicePx, Device>> {
        let taken = Size::new(DevicePx(size.width.0), DevicePx(size.height.0));
        *self.size.lock().expect("the size is not poisoned") = taken;
        Some(taken)
    }

    fn set_min_size(&self, _size: Option<Size<CssPx, Css>>) {}

    fn set_max_size(&self, _size: Option<Size<CssPx, Css>>) {}

    fn request_redraw(&self) {
        self.redraws.fetch_add(1, Ordering::Relaxed);
    }

    fn pre_present_notify(&self) {}

    fn set_title(&self, _title: &str) {}

    fn set_visible(&self, visible: bool) {
        self.visible.store(visible, Ordering::Relaxed);
    }

    fn set_decorated(&self, _decorated: bool) {}

    fn set_resizable(&self, _resizable: bool) {}

    fn set_maximized(&self, _maximized: bool) {}

    fn set_minimized(&self, _minimized: bool) {}

    fn set_fullscreen(&self, _mode: Option<FullscreenMode>) {}

    fn set_cursor(&self, _cursor: CursorStyle) {}

    fn set_text_input(&self, _state: Option<TextInput>) {}

    fn push_a11y_update(&self, build: &mut dyn FnMut() -> TreeUpdate) {
        // A headless surface always has a listener, because the test *is* the listener.
        let _ = build();
        self.a11y_updates.fetch_add(1, Ordering::Relaxed);
    }
}
