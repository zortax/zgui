//! Bytes from a URL, and the fonts a page declares.

use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

/// The bytes at `url`.
///
/// # Errors
///
/// Returns a message when the request fails or the server answers with an error status.
pub async fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let window = web_sys::window().ok_or_else(|| "this context has no window".to_owned())?;
    let response: web_sys::Response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|error| format!("{url}: {error:?}"))?
        .dyn_into()
        .map_err(|_| format!("{url}: the page answered with no response"))?;
    if !response.ok() {
        return Err(format!("{url}: status {}", response.status()));
    }
    let buffer = response
        .array_buffer()
        .map_err(|error| format!("{url}: {error:?}"))?;
    let buffer = JsFuture::from(buffer)
        .await
        .map_err(|error| format!("{url}: {error:?}"))?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

/// The font files the page preloads, in document order.
///
/// A page names a font file with a standard preload link, and the application registers each
/// one before its first frame:
///
/// ```html
/// <link rel="preload" as="fetch" type="font/ttf" href="fonts/Inter.ttf" crossorigin>
/// ```
///
/// The browser starts the download while the application module loads, and the request made
/// through [`fetch`] is answered from that download. The link says `as="fetch"` because that is
/// how the application reads the file: a browser gives a download preloaded `as="font"` only to
/// its own text engine.
pub fn page_fonts() -> Vec<String> {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return Vec::new();
    };
    let Ok(links) =
        document.query_selector_all("link[rel~=preload][as=fetch][type^=\"font/\"][href]")
    else {
        return Vec::new();
    };
    (0..links.length())
        .filter_map(|index| links.item(index))
        .filter_map(|node| node.dyn_into::<web_sys::Element>().ok())
        .filter_map(|link| {
            // The property is the resolved URL. The attribute is what the page wrote.
            js_sys::Reflect::get(&link, &"href".into())
                .ok()
                .and_then(|href| href.as_string())
        })
        .collect()
}
