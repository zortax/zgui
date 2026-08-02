//! The windowing backend's shape, asserted without a windowing system.

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

use accesskit::TreeUpdate;
use zgui_geom::{Css, CssPx, Device, DevicePx, Size};

use crate::backends::headless::surface::OffscreenSurface;
use crate::surface::{CursorStyle, FullscreenMode, GpuSurface, Surface, SurfaceId, TextInput};

/// A surface that carries native handles, as a windowing backend's does.
///
/// The handles are reported unavailable, because a real one cannot be manufactured without a real
/// window. What is being checked is that a type of this *shape* satisfies the graphics contract at
/// all — that the supertraits compose, that the trait object is formable, and that the surface can
/// hand itself out through the graphics accessor.
#[derive(Debug)]
struct NativeSurface {
    inner: OffscreenSurface,
}

impl HasWindowHandle for NativeSurface {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

impl HasDisplayHandle for NativeSurface {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

impl Surface for NativeSurface {
    fn id(&self) -> SurfaceId {
        self.inner.id()
    }

    fn size(&self) -> Size<DevicePx, Device> {
        self.inner.size()
    }

    fn scale_factor(&self) -> f64 {
        2.0
    }

    fn refresh_rate_millihertz(&self) -> Option<u32> {
        Some(59_940)
    }

    fn request_size(&self, size: Size<CssPx, Css>) -> Option<Size<DevicePx, Device>> {
        self.inner.request_size(size)
    }

    fn set_min_size(&self, size: Option<Size<CssPx, Css>>) {
        self.inner.set_min_size(size);
    }

    fn set_max_size(&self, size: Option<Size<CssPx, Css>>) {
        self.inner.set_max_size(size);
    }

    fn request_redraw(&self) {
        self.inner.request_redraw();
    }

    fn pre_present_notify(&self) {}

    fn set_title(&self, title: &str) {
        self.inner.set_title(title);
    }

    fn set_visible(&self, visible: bool) {
        self.inner.set_visible(visible);
    }

    fn set_decorated(&self, decorated: bool) {
        self.inner.set_decorated(decorated);
    }

    fn set_resizable(&self, resizable: bool) {
        self.inner.set_resizable(resizable);
    }

    fn set_maximized(&self, maximized: bool) {
        self.inner.set_maximized(maximized);
    }

    fn set_minimized(&self, minimized: bool) {
        self.inner.set_minimized(minimized);
    }

    fn set_fullscreen(&self, mode: Option<FullscreenMode>) {
        self.inner.set_fullscreen(mode);
    }

    fn set_cursor(&self, cursor: CursorStyle) {
        self.inner.set_cursor(cursor);
    }

    fn set_text_input(&self, state: Option<TextInput>) {
        self.inner.set_text_input(state);
    }

    fn push_a11y_update(&self, build: &mut dyn FnMut() -> TreeUpdate) {
        self.inner.push_a11y_update(build);
    }

    fn gpu(&self) -> Option<&dyn GpuSurface> {
        Some(self)
    }
}

/// Compiles only if `T` can be a shared, thread-safe surface behind a pointer.
fn assert_shareable_surface<T: Surface>() {}

/// Compiles only if `T` satisfies the graphics contract.
fn assert_gpu_surface<T: GpuSurface>() {}

#[cfg(test)]
mod tests {
    use super::{NativeSurface, assert_gpu_surface, assert_shareable_surface};
    use crate::backends::headless::surface::OffscreenSurface;
    use crate::surface::{Surface, SurfaceId};
    use zgui_geom::{DevicePx, Size};

    fn native() -> NativeSurface {
        NativeSurface {
            inner: OffscreenSurface::new(
                SurfaceId::new(1),
                Size::new(DevicePx(0.0), DevicePx(0.0)),
            ),
        }
    }

    #[test]
    fn a_windowing_surface_offers_them_and_is_the_same_trait_object() {
        let native = native();
        let surface: &dyn Surface = &native;
        let gpu = surface.gpu().expect("a windowing surface has handles");
        assert!(gpu.window_handle().is_err());
        assert!(gpu.display_handle().is_err());
        assert_eq!(gpu.id(), SurfaceId::new(1));
        assert_eq!(surface.refresh_rate_millihertz(), Some(59_940));
    }

    #[test]
    fn the_graphics_accessor_is_the_only_difference_between_the_two_shapes() {
        // Both are the same trait behind the same pointer; one answers the graphics question and
        // the other declines, and nothing above the trait can tell them apart any other way.
        let native = native();
        let offscreen =
            OffscreenSurface::new(SurfaceId::new(2), Size::new(DevicePx(0.0), DevicePx(0.0)));
        let shapes: [&dyn Surface; 2] = [&native, &offscreen];
        assert!(shapes[0].gpu().is_some());
        assert!(shapes[1].gpu().is_none());
    }

    #[test]
    fn both_surface_shapes_satisfy_the_compile_time_contracts() {
        assert_shareable_surface::<OffscreenSurface>();
        assert_shareable_surface::<NativeSurface>();
        assert_gpu_surface::<NativeSurface>();
    }
}
