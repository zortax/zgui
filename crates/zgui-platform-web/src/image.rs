//! Images, decoded by the browser.
//!
//! A browser decodes an image off the thread the page runs on, and it reads every format the
//! page itself can show. So the browser loads an image `src`, scales it to the size it is shown
//! at, and hands over the pixels. The application fetches and decodes nothing.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::{HtmlImageElement, ImageBitmap, OffscreenCanvas, OffscreenCanvasRenderingContext2d};

/// The loaded image at `url`.
#[cfg(target_arch = "wasm32")]
async fn load(url: &str) -> Result<HtmlImageElement, String> {
    let image = HtmlImageElement::new().map_err(|_| format!("{url}: the page refused an image"))?;
    // The pixels are read back, which a browser allows only for an image it was allowed to share.
    image.set_cross_origin(Some("anonymous"));
    image.set_src(url);
    JsFuture::from(image.decode())
        .await
        .map_err(|_| format!("{url}: the image did not load or is no image"))?;
    Ok(image)
}

/// The pixel extent of the image at `url`, as width and height.
///
/// # Errors
///
/// Returns a message when the image does not load or is no image.
#[cfg(target_arch = "wasm32")]
pub async fn image_size(url: &str) -> Result<(u32, u32), String> {
    let image = load(url).await?;
    Ok((image.natural_width(), image.natural_height()))
}

/// The image at `url`, scaled down until its long edge fits `long_edge`: its width, its height
/// and its texels, which are premultiplied RGBA, top row first.
///
/// # Errors
///
/// Returns a message when the image does not load, is no image, or cannot be read back.
#[cfg(target_arch = "wasm32")]
pub async fn image_pixels(url: &str, long_edge: u32) -> Result<(u32, u32, Vec<u8>), String> {
    let image = load(url).await?;
    let (width, height) = fit(image.natural_width(), image.natural_height(), long_edge);
    if width == 0 || height == 0 {
        return Ok((0, 0, Vec::new()));
    }
    let refused = |what: &str| format!("{url}: {what}");
    let window = web_sys::window().ok_or_else(|| refused("this context has no window"))?;
    let options = web_sys::ImageBitmapOptions::new();
    options.set_resize_width(width);
    options.set_resize_height(height);
    options.set_resize_quality(web_sys::ResizeQuality::High);
    let bitmap: ImageBitmap = JsFuture::from(
        window
            .create_image_bitmap_with_html_image_element_and_image_bitmap_options(&image, &options)
            .map_err(|_| refused("the browser refused to scale the image"))?,
    )
    .await
    .map_err(|_| refused("the browser could not scale the image"))?
    .dyn_into()
    .map_err(|_| refused("the browser answered with no bitmap"))?;

    let canvas =
        OffscreenCanvas::new(width, height).map_err(|_| refused("the page refused a canvas"))?;
    let context: OffscreenCanvasRenderingContext2d = canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|context| context.dyn_into().ok())
        .ok_or_else(|| refused("the page refused a drawing context"))?;
    context
        .draw_image_with_image_bitmap(&bitmap, 0.0, 0.0)
        .map_err(|_| refused("the bitmap could not be drawn"))?;
    bitmap.close();
    let mut texels = context
        .get_image_data(0.0, 0.0, f64::from(width), f64::from(height))
        .map_err(|_| refused("the pixels could not be read back"))?
        .data()
        .0;
    premultiply(&mut texels);
    Ok((width, height, texels))
}

/// The extent of an image of `width` by `height` after its long edge was fitted to `long_edge`.
/// An image that fits already keeps its extent.
pub(crate) fn fit(width: u32, height: u32, long_edge: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= long_edge || longest == 0 {
        return (width, height);
    }
    let scale = f64::from(long_edge) / f64::from(longest);
    let scaled = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    (scaled(width), scaled(height))
}

/// Multiplies the colour of each texel by its alpha. A canvas hands its pixels over unmultiplied.
pub(crate) fn premultiply(texels: &mut [u8]) {
    for texel in texels.chunks_exact_mut(4) {
        let alpha = u32::from(texel[3]);
        if alpha == 255 {
            continue;
        }
        for channel in &mut texel[..3] {
            *channel = ((u32::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fit, premultiply};

    #[test]
    fn an_image_is_scaled_by_its_long_edge_and_never_up() {
        assert_eq!(fit(4000, 2000, 1000), (1000, 500));
        assert_eq!(fit(300, 900, 300), (100, 300));
        assert_eq!(fit(640, 480, 2048), (640, 480));
        assert_eq!(fit(5000, 3, 100), (100, 1));
    }

    #[test]
    fn colour_is_multiplied_by_alpha_and_an_opaque_texel_is_left_alone() {
        let mut texels = [200, 100, 50, 255, 200, 100, 50, 128, 9, 9, 9, 0];
        premultiply(&mut texels);
        assert_eq!(texels, [200, 100, 50, 255, 100, 50, 25, 128, 0, 0, 0, 0]);
    }
}
