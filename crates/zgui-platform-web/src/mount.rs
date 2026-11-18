//! Where the canvas comes from.

use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;
use zgui_platform::PlatformError;

/// Where an application draws in a page.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub enum Mount {
    /// A new canvas that fills the viewport. The application owns the page: its title follows
    /// the window title, and the page does not scroll.
    #[default]
    Viewport,
    /// A canvas of the page. The page sets its size with CSS.
    Canvas(HtmlCanvasElement),
    /// The first canvas that matches a CSS selector.
    Selector(String),
}

impl Mount {
    /// A canvas of the page.
    pub const fn canvas(canvas: HtmlCanvasElement) -> Self {
        Self::Canvas(canvas)
    }

    /// The first canvas that matches the CSS selector `selector`.
    pub fn selector(selector: impl Into<String>) -> Self {
        Self::Selector(selector.into())
    }

    /// Returns `true` where the application owns the whole page.
    pub const fn owns_page(&self) -> bool {
        matches!(self, Self::Viewport)
    }
}

/// The canvas `mount` names, created where it names none.
///
/// # Errors
///
/// Returns [`PlatformError::SurfaceCreation`] when there is no document, when a selector matches
/// no canvas, or when the page refuses a new element.
pub fn canvas(mount: &Mount) -> Result<HtmlCanvasElement, PlatformError> {
    let refused = |what: &str| PlatformError::SurfaceCreation(what.to_owned());
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| refused("this context has no document"))?;
    let canvas = match mount {
        Mount::Canvas(canvas) => canvas.clone(),
        Mount::Selector(selector) => document
            .query_selector(selector)
            .ok()
            .flatten()
            .and_then(|element| element.dyn_into::<HtmlCanvasElement>().ok())
            .ok_or_else(|| {
                PlatformError::SurfaceCreation(format!("no canvas matches `{selector}`"))
            })?,
        Mount::Viewport => {
            let body = document
                .body()
                .ok_or_else(|| refused("the document has no body"))?;
            let canvas = document
                .create_element("canvas")
                .ok()
                .and_then(|element| element.dyn_into::<HtmlCanvasElement>().ok())
                .ok_or_else(|| refused("the page refused a canvas element"))?;
            let page = body.style();
            let _ = page.set_property("margin", "0");
            let _ = page.set_property("overflow", "hidden");
            let style = canvas.style();
            let _ = style.set_property("position", "fixed");
            let _ = style.set_property("inset", "0");
            let _ = style.set_property("width", "100%");
            let _ = style.set_property("height", "100%");
            let _ = style.set_property("display", "block");
            body.append_child(&canvas)
                .map_err(|_| refused("the page refused a canvas element"))?;
            canvas
        }
    };
    let style = canvas.style();
    // The framework reads every touch and every wheel itself.
    let _ = style.set_property("touch-action", "none");
    let _ = style.set_property("outline", "none");
    if !canvas.has_attribute("tabindex") {
        let _ = canvas.set_attribute("tabindex", "0");
    }
    Ok(canvas)
}

/// The value of `name` in the page's query string.
///
/// A page selects a renderer with it: `?renderer=webgl2`.
pub fn query(name: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search)
        .ok()?
        .get(name)
}

/// Whether text on this page's display is likely to look right with per-channel coverage.
///
/// A page cannot ask how the subpixels of a display are arranged, so this is the rule browsers
/// follow for their own text: a desktop display of ordinary density, on a system that draws its
/// own text that way. A phone, a tablet and a display of double density or more draw text with
/// one coverage for each pixel, and so does an Apple system.
pub fn subpixel_text_suits() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let fine_pointer = window
        .match_media("(pointer: fine)")
        .ok()
        .flatten()
        .is_some_and(|query| query.matches());
    fine_pointer && !is_apple(&window) && window.device_pixel_ratio() < 2.0
}

/// Whether the browser runs on an Apple system, where the command key is the shortcut modifier.
pub(crate) fn is_apple(window: &web_sys::Window) -> bool {
    window.navigator().platform().is_ok_and(|platform| {
        ["Mac", "iPhone", "iPad", "iPod"]
            .iter()
            .any(|name| platform.contains(name))
    })
}
