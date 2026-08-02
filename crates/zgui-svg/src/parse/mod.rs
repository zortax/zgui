//! Reading an SVG document, and the one boundary the parser is behind.
//!
//! Nothing outside this module names the parser's types. A [`Document`] is Béziers, colours and
//! stroke styles in the same vocabularies the rest of the framework already speaks, so the two
//! path rasterisers this framework ships both draw a document without either of them knowing that
//! SVG exists — and swapping the parser would change these files and no others.

mod clip;
mod geometry;
mod inherit;
mod paint;
mod stroke;
mod text;
mod walk;

use crate::document::Document;

/// Why a document could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The bytes are not a well-formed SVG document.
    #[error("not a readable SVG document: {0}")]
    Malformed(String),
    /// The document is well formed but names no area to draw in.
    #[error("the document has no usable size")]
    Sizeless,
}

/// Reads one SVG document.
///
/// The result is in the document's own coordinates: [`Document::view_box`] is the rectangle the
/// outlines were written inside, already reduced by the document's `viewBox` and its
/// `preserveAspectRatio` to a plain rectangle at the origin, and fitting that onto an element's
/// box is the caller's. Nothing here knows how large the drawing will be drawn, which is why one
/// parse serves every size it is ever drawn at.
///
/// ```
/// let document = zgui_svg::parse(
///     r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12">
///          <rect width="24" height="12" fill="#ff0000"/>
///        </svg>"##,
/// )
/// .expect("a readable document");
///
/// assert_eq!(document.view_box(), [0.0, 0.0, 24.0, 12.0]);
/// assert_eq!(document.shapes().len(), 1);
/// assert!(!document.is_inherited(), "a document with its own colour keeps it");
/// ```
///
/// # What is read
///
/// Outlines with their fills and strokes, fill and clip rules, stroke widths, caps, joins, miter
/// limits and dashes; groups with their transforms, their opacity and their clip paths; linear and
/// radial gradients with their stops and all three spread methods; `use`, `symbol` and markers,
/// which the parser resolves into outlines before this sees them.
///
/// # What is not
///
/// `<text>`, which would need a second font stack beside the one this framework already has, is
/// dropped; `<image>`, filters, masks, patterns and blend modes are dropped too. Each is counted
/// in [`Document::unsupported`] rather than being passed over silently, so a caller can tell a
/// document this cannot draw from one that draws nothing.
///
/// Group opacity is folded into the alpha of the shapes inside the group rather than composited as
/// a layer, so overlapping children of a translucent group show through one another where a browser
/// would draw the group once and fade the result.
pub fn parse(source: &str) -> Result<Document, Error> {
    let first = read(source, inherit::first_sheet())?;
    let (shapes, mut unsupported) = walk::document(&first);
    unsupported.text += text::count(source);
    let second = read(source, inherit::second_sheet())?;
    let (compare, _) = walk::document(&second);
    let size = first.size();
    Ok(Document::new(
        (f64::from(size.width()), f64::from(size.height())),
        inherit::merge(shapes, &compare),
        unsupported,
    ))
}

/// Parses `source` once, with `sheet` deciding what `currentColor` resolves to.
fn read(source: &str, sheet: String) -> Result<usvg::Tree, Error> {
    let options = usvg::Options {
        style_sheet: Some(sheet),
        ..usvg::Options::default()
    };
    usvg::Tree::from_str(source, &options).map_err(|failure| match failure {
        usvg::Error::InvalidSize => Error::Sizeless,
        other => Error::Malformed(other.to_string()),
    })
}
