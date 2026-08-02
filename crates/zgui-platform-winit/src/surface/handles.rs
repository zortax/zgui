//! The window as a graphics API sees it.

use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

use crate::surface::WinitSurface;

/// The native window handle, forwarded from the window itself.
///
/// This and its display twin are the whole of what a graphics API needs from a window, and they are
/// the only part of a window this framework's renderer ever sees. Everything else it is told — the
/// size to allocate for, the scale to draw at, when a frame is wanted — arrives through the
/// platform contract, in the contract's own vocabulary.
///
/// The handle borrows the window, and the window is kept alive by the surface that holds it, so a
/// graphics API given a shared handle to the surface holds the window open for as long as it draws.
impl HasWindowHandle for WinitSurface {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.window().window_handle()
    }
}

/// The native display handle, forwarded from the window itself.
impl HasDisplayHandle for WinitSurface {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.window().display_handle()
    }
}
