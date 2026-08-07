//! One fragment's own primitives, in CSS 2.1 Appendix E's order.
//!
//! The sequence is the specification's and it is stated once, here, rather than implied by the
//! order of a dozen calls scattered through the walk:
//!
//! 1. the shadows the box casts outwards, behind everything;
//! 2. its background, and its border over that;
//! 3. the shadows it casts inwards, over the background;
//! 4. its content — glyphs, an image, its own outlines, a scrollbar part;
//! 5. and, after the box's *descendants* rather than here, its outline.
//!
//! Step five is why the outline is a separate function: it belongs after the subtree, and a function
//! that emitted it beside the background would sort a box's focus ring underneath its own text.

use zgui_layout::{Fragment, FragmentKind};
use zgui_scene::{Scene, VectorId};

use crate::content::vectors::VectorMaskSource;
use crate::content::vectors::{Placement as DrawingPlacement, VectorSource};
use crate::emit::box_::{self, BoxPlacement};
use crate::emit::highlight::{self, HighlightLayer, HighlightRequest, HighlightSource};
use crate::emit::replaced::{ReplacedPlacement, Source};
use crate::emit::scrollbar::{self, ScrollbarPaint};
use crate::emit::text::{self, GlyphSource, TextPlacement};
use crate::emit::vector::{self, VectorPlacement};
use crate::lower::PaintStyle;

/// Where a replaced box's content comes from.
///
/// A fragment names the content it draws and nothing about it; what that name resolves to — an
/// atlas tile for a decoded image, a texture handle for a video frame — belongs to whoever decoded
/// it. Keeping it a seam is what lets the paint stage be exercised with no decoder.
pub trait ReplacedSource {
    /// Where the content named by `id` lives, or nothing if it has not been decoded yet.
    fn source(&self, id: zgui_dom::host::ReplacedId) -> Option<Source>;
}

/// A replaced-content source with nothing in it.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoReplaced;

impl ReplacedSource for NoReplaced {
    fn source(&self, _id: zgui_dom::host::ReplacedId) -> Option<Source> {
        None
    }
}

/// Everything one fragment needs in order to be emitted.
pub struct Emission<'a> {
    /// The lowered style the fragment's box carries.
    pub style: &'a PaintStyle,
    /// Where the box's decorations go.
    pub box_placement: BoxPlacement,
    /// Where its text goes.
    pub text_placement: TextPlacement,
    /// The alpha folded into every colour, which is one unless a group's opacity was folded in.
    pub alpha: f32,
    /// The text decorations in force at this fragment, outermost first, already faded.
    ///
    /// A list rather than the fragment's own style's value: a decoration belongs to the box that
    /// declared it and is drawn across that box's in-flow descendants, so what a line box draws is
    /// what its ancestors contributed and not what its own anonymous box says.
    pub decorations: &'a [crate::emit::text::DecorationStyle],
    /// The ramp painting the text at this fragment, if a box above it asked for one.
    ///
    /// A ramp rather than the fragment's own style's, for the reason the decorations are a list:
    /// `background-image` does not inherit, and a line box belongs to an anonymous box generated
    /// below whatever declared it.
    pub text_fill: Option<&'a crate::lower::background::GradientSpec>,
    /// Where glyphs come from.
    pub glyphs: &'a dyn GlyphSource,
    /// Where the caret and the selection bands drawn with a line come from.
    pub highlights: &'a dyn HighlightSource,
    /// Where replaced content comes from.
    pub replaced: &'a dyn ReplacedSource,
    /// Where the outlines an element draws come from.
    pub vectors: &'a dyn VectorSource,
    /// Where eligible solid paths get cached monochrome coverage.
    pub vector_masks: &'a dyn VectorMaskSource,
    /// Where a custom element's painting comes from.
    pub custom: &'a dyn crate::content::custom::CustomPaintSource,
    /// The custom-element reference the fragment's box captured, when it has one.
    pub custom_reference: Option<(u32, u16, u16)>,
    /// How many device pixels one CSS pixel is, which is what a drawing with no space of its own is
    /// scaled by.
    pub scale: f32,
    /// How a scrollbar is painted.
    pub scrollbars: ScrollbarPaint,
}

/// Emits one fragment's own primitives, and returns how many were pushed.
pub fn fragment(scene: &mut Scene, fragment: &Fragment, emission: &Emission<'_>) -> usize {
    if vanished(emission.alpha) {
        return 0;
    }
    let style = faded(emission.style, emission.alpha);
    let mut pushed = 0;
    // A box whose decorations draw nothing at all is the overwhelming majority of containers, and
    // asking once is cheaper than three calls that each resolve a paint and push no primitive.
    if !style.paints_nothing() {
        pushed += box_::outer_shadows(scene, &style, emission.box_placement);
        pushed += box_::background_and_border(scene, &style, emission.box_placement);
        pushed += box_::inset_shadows(scene, &style, emission.box_placement);
    }
    pushed += content(scene, fragment, &style, emission);
    pushed
}

/// Emits a fragment's outline, which is drawn after its descendants.
pub fn outline(scene: &mut Scene, emission: &Emission<'_>) -> usize {
    if vanished(emission.alpha) {
        return 0;
    }
    let style = faded(emission.style, emission.alpha);
    if !style.visible {
        return 0;
    }
    box_::outline(scene, &style, emission.box_placement)
}

/// Whether a folded alpha has taken everything under it to nothing.
///
/// Every colour a fragment paints with is multiplied by this number, so at zero each one of them is
/// fully transparent and each primitive composites its target onto itself. The primitives are
/// therefore not pushed at all, for the same reason a fully transparent background is not: a
/// primitive that cannot change a pixel still costs an entry in the display list, a rectangle in
/// the damage arithmetic and — for a drawing, which is composited from a scratch of its own — a
/// whole rasterisation pass.
///
/// This is what makes a control that carries several marks and reveals one cost what the one it
/// shows costs, rather than what all of them cost.
fn vanished(alpha: f32) -> bool {
    alpha <= 0.0
}

/// Emits whatever a fragment draws inside its box decorations.
fn content(
    scene: &mut Scene,
    fragment: &Fragment,
    style: &PaintStyle,
    emission: &Emission<'_>,
) -> usize {
    if !style.visible {
        return 0;
    }
    match fragment.kind {
        FragmentKind::Box => 0,
        FragmentKind::Line { paragraph, line } => {
            // The selection under the glyphs, the caret over them. Neither order is a preference:
            // a band drawn over opaque text hides the text it is meant to mark, and a caret drawn
            // under it disappears inside whichever letter it is sitting on.
            let mut pushed = marks(scene, emission, paragraph, line, HighlightLayer::Behind);
            pushed += text::emit(
                scene,
                emission.glyphs,
                paragraph,
                line,
                style,
                text::Inherited {
                    text_fill: emission.text_fill,
                    decorations: emission.decorations,
                },
                emission.text_placement,
            );
            pushed + marks(scene, emission, paragraph, line, HighlightLayer::InFront)
        }
        // A run is a style-uniform span *within* a line, and a fragment tree that splits lines that
        // far is what a rich-text editor needs rather than what a document produces. Until something
        // produces one, a run draws through the same path its line would.
        FragmentKind::TextRun { paragraph, run } => text::emit(
            scene,
            emission.glyphs,
            paragraph,
            run,
            style,
            text::Inherited {
                text_fill: emission.text_fill,
                decorations: emission.decorations,
            },
            emission.text_placement,
        ),
        FragmentKind::Replaced { content } => match emission.replaced.source(content) {
            Some(source) => crate::emit::replaced::emit(
                scene,
                source,
                ReplacedPlacement {
                    content_box: fragment.content_box,
                    radii: box_::padding_radii(
                        emission.box_placement.radii,
                        emission.box_placement.border,
                    ),
                    clip: emission.box_placement.clip,
                    transform: emission.box_placement.transform,
                    opacity: emission.alpha,
                },
            ),
            None => 0,
        },
        // The outlines are drawn inside the box's own decorations and under the same clip and
        // transform, so a drawing sorts, clips and moves exactly like a background does — and an
        // element that draws is otherwise an ordinary element with an ordinary box.
        FragmentKind::Vector => {
            let Some(node) = fragment.node else {
                return 0;
            };
            let Some(drawing) = emission.vectors.drawing(
                node,
                DrawingPlacement {
                    content_box: fragment.content_box,
                    scale: emission.scale,
                },
            ) else {
                return 0;
            };
            vector::draw_with_masks(
                scene,
                VectorId(fragment.key.index()),
                &drawing.shapes,
                style.shape,
                emission.vector_masks,
                VectorPlacement {
                    clip: emission.box_placement.clip,
                    transform: emission.box_placement.transform,
                    scale: emission.scale,
                },
            )
        }
        // A custom element's primitives land here for the reason the vector arm's do: inside the
        // box's own decorations, under its clip and transform, before its descendants — sorting,
        // clipping and moving exactly like a background, whoever produced them.
        FragmentKind::Custom => {
            let Some((token, _, _)) = emission.custom_reference else {
                return 0;
            };
            let mut painter = crate::content::custom::ScenePainter {
                scene,
                content_box: fragment.content_box,
                clip: emission.box_placement.clip,
                transform: emission.box_placement.transform,
                alpha: emission.alpha,
                scale: emission.scale,
                shape_paint: style.shape,
                vector_masks: emission.vector_masks,
                vector_id: VectorId(fragment.key.index()),
                shapes_pushed: 0,
                pushed: 0,
            };
            emission.custom.paint(token, &mut painter);
            painter.pushed
        }
        FragmentKind::Scrollbar { part, .. } => scrollbar::emit(
            scene,
            part,
            fragment.border_box,
            emission.scrollbars,
            emission.box_placement.clip,
        ),
    }
}

/// Emits the caret and selection rectangles of one line that belong on `layer`.
fn marks(
    scene: &mut Scene,
    emission: &Emission<'_>,
    paragraph: zgui_layout::fragment::ParagraphId,
    line: u16,
    layer: HighlightLayer,
) -> usize {
    highlight::emit(
        scene,
        emission.highlights,
        paragraph,
        line,
        layer,
        HighlightRequest {
            origin: emission.text_placement.line.origin,
            scale: emission.scale,
        },
        emission.text_placement.clip,
        emission.text_placement.transform,
        emission.alpha,
    )
}

/// The same style with every colour's alpha scaled.
///
/// This is how a folded group opacity is applied: multiplying it into each primitive's own paint
/// produces the same pixels as compositing the subtree once, exactly when no two primitives in it
/// overlap — and that condition is decided over the fragment tree before this is reached.
fn faded(style: &PaintStyle, alpha: f32) -> PaintStyle {
    if alpha >= 1.0 {
        return style.clone();
    }
    let mut faded = style.clone();
    faded.color = faded.color.with_alpha(faded.color.alpha() * alpha);
    faded.background.color = faded
        .background
        .color
        .with_alpha(faded.background.color.alpha() * alpha);
    for layer in faded
        .background
        .layers
        .iter_mut()
        .chain(faded.text_fill.iter_mut())
    {
        for stop in &mut layer.stops {
            stop.color = stop.color.with_alpha(stop.color.alpha() * alpha);
        }
    }
    for color in &mut faded.border.colors {
        *color = color.with_alpha(color.alpha() * alpha);
    }
    for shadow in faded
        .shadows
        .iter_mut()
        .chain(faded.text_shadows.iter_mut())
    {
        shadow.color = shadow.color.with_alpha(shadow.color.alpha() * alpha);
    }
    if let Some(outline) = &mut faded.outline {
        outline.color = outline.color.with_alpha(outline.color.alpha() * alpha);
    }
    faded.decoration.color = faded
        .decoration
        .color
        .with_alpha(faded.decoration.color.alpha() * alpha);
    faded.shape.fill = faded
        .shape
        .fill
        .with_alpha(faded.shape.fill.alpha() * alpha);
    faded.shape.stroke = faded
        .shape
        .stroke
        .map(|color| color.with_alpha(color.alpha() * alpha));
    faded
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;

    use super::faded;
    use crate::lower::lower;

    #[test]
    fn folding_an_alpha_scales_every_colour_a_style_carries() {
        let mut style = lower(&StyleDraft::initial().build(), 1.0);
        style.background.color = zgui_color::Color::srgb(1.0, 0.0, 0.0, 0.8);
        style.border.colors = [zgui_color::Color::srgb(0.0, 0.0, 1.0, 1.0); 4];
        let half = faded(&style, 0.5);
        assert_eq!(half.background.color.alpha(), 0.4);
        assert_eq!(half.border.colors[0].alpha(), 0.5);
        assert_eq!(half.color.alpha(), 0.5);
    }

    #[test]
    fn folding_a_full_alpha_changes_nothing() {
        let style = lower(&StyleDraft::initial().build(), 1.0);
        assert_eq!(faded(&style, 1.0), style);
    }
}
