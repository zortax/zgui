//! What a run reads back off a window: how big the document is, and where things are in it.

use zgui::geom::{Css, CssPx, Device, DevicePx, Point};
use zgui::prelude::*;

/// How big the document is: boxes, fragments, and the fragments that carry a box of their own.
pub(crate) fn document(window: &zgui::runtime::Window) -> (usize, usize) {
    let layout = window.layout().borrow();
    let keys = layout.keys();
    let fragments: usize = keys
        .iter()
        .map(|key| layout.fragments_of_box(*key).len())
        .sum();
    let anonymous = keys
        .iter()
        .filter(|key| layout.get(**key).is_none_or(|node| node.source.is_none()))
        .count();
    println!("  anonymous boxes: {anonymous} of {}", keys.len());
    (keys.len(), fragments)
}

/// The most of each kind of drawn thing any one full repaint of the session held.
///
/// # Why a differential has to say this
///
/// "The two pictures agreed" is a claim about a document, and a document that draws nothing agrees
/// with itself perfectly. The blank check catches the extreme of that — a repaint that emitted no
/// primitive at all — but not the shape of it that matters here: a session in which the text
/// survived and every curve, every image and every vector pass quietly did not.
///
/// So the run reports what it actually had in front of it. Each number is the largest a single full
/// repaint reached, because a repaint is one whole picture and the largest is the fullest page the
/// session ever put on the screen. A kind that reads zero at a document size that contains it is a
/// kind that was never drawn, whatever the comparison said.
#[derive(Default)]
pub(crate) struct Painted {
    /// Glyphs drawn through the atlas, monochrome or subpixel.
    glyphs: usize,
    /// Images and icons drawn as a coloured sprite.
    sprites: usize,
    /// Runs and artwork drawn as filled curves.
    curves: usize,
    /// Vector passes, which is what an SVG document is composited through.
    passes: usize,
    /// Fills that are a gradient rather than a colour.
    gradients: usize,
    /// Things drawn under a transform, which is what a rotated run is.
    transformed: usize,
}

impl Painted {
    /// Takes the counts of one full repaint's display list.
    pub(crate) fn saw(&mut self, list: &str) {
        let count = |needle: &str| list.lines().filter(|line| line.contains(needle)).count();
        self.glyphs = self
            .glyphs
            .max(count("mono_sprite ") + count("subpixel_sprite "));
        self.sprites = self.sprites.max(count("color_sprite "));
        self.curves = self.curves.max(count("vector order"));
        self.passes = self.passes.max(count("pass "));
        // A gradient paint is printed as its shape and its stops, never as the word "gradient", so
        // the stop list is what names one: every gradient has one and nothing else does.
        self.gradients = self.gradients.max(count("stops=["));
        self.transformed = self.transformed.max(count("transform=#"));
    }
}

impl core::fmt::Display for Painted {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "painted[glyphs={} sprites={} curves={} passes={} gradients={} transformed={}]",
            self.glyphs, self.sprites, self.curves, self.passes, self.gradients, self.transformed
        )
    }
}

/// Where every scrolled container is composed, as one line.
///
/// The *composed* position and not the clamped one, because it is what the fragment pass reads and
/// therefore the only one a picture can be a function of: a container held past its end by a gesture
/// is drawn where the gesture holds it, and comparing the clamped number would call that agreement.
pub(crate) fn scroll_signature(window: &zgui::runtime::Window) -> String {
    let scroll = window.scroll().borrow();
    let mut places: Vec<String> = scroll
        .composed()
        .iter()
        .filter(|(_, at)| at.x.0 != 0.0 || at.y.0 != 0.0)
        .map(|(_, at)| format!("({:.2},{:.2})", at.x.0, at.y.0))
        .collect();
    places.sort();
    if places.is_empty() {
        "origin".to_owned()
    } else {
        places.join(" ")
    }
}

/// The centre of every 34x34 box in the window, which is what the probe swatches are.
pub(crate) fn swatch_centres(window: &zgui::runtime::Window) -> Vec<Point<CssPx, Css>> {
    let layout = window.layout().borrow();
    let mut found = Vec::new();
    for key in layout.keys() {
        for fragment in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*fragment) else {
                continue;
            };
            let border: zgui::geom::Rect<DevicePx, Device> = fragment.border_box;
            let width = border.size.width.0;
            let height = border.size.height.0;
            if (width - 34.0).abs() < 0.5 && (height - 34.0).abs() < 0.5 {
                found.push(Point::new(
                    CssPx(border.origin.x.0 + width / 2.0),
                    CssPx(border.origin.y.0 + height / 2.0),
                ));
            }
        }
    }
    found.sort_by(|a, b| a.y.0.total_cmp(&b.y.0).then(a.x.0.total_cmp(&b.x.0)));
    found.truncate(4);
    found.sort_by(|a, b| a.x.0.total_cmp(&b.x.0));
    found
}

/// The centre of the first element carrying `data-testid="<name>"`, in CSS pixels.
///
/// The document is walked rather than the layout store because what a step wants to press is a
/// named control, and the name lives on the element. A control that has not been built yet, or one
/// whose box was never laid out, has no centre and the caller is told so rather than given a point
/// in the middle of something else.
pub(crate) fn testid_centre(
    window: &zgui::runtime::Window,
    name: &str,
) -> Option<Point<CssPx, Css>> {
    let dom = window.dom();
    let attribute = zgui::view::AttrName::new("data-testid");
    let mut stack = vec![dom.root_node()];
    while let Some(node) = stack.pop() {
        if dom.attribute(node, attribute).as_deref() == Some(name) {
            let border = window.host().border_box(node)?;
            return Some(Point::new(
                CssPx(border.origin.x.0 + border.size.width.0 / 2.0),
                CssPx(border.origin.y.0 + border.size.height.0 / 2.0),
            ));
        }
        stack.extend(dom.children(node));
    }
    None
}
