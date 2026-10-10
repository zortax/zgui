//! Turning one paragraph's content into glyphs.

use std::sync::Arc;

use zgui_geom::CssPx;
use zgui_text::{
    Brush, ContentWidths, InlineBoxGeometry, ParagraphContent, ParagraphKey, ShapedParagraph,
    StrutMetrics, StyledRun,
};
use zgui_text_style::TextStyle;

use crate::direction::Controls;
use crate::shape::brush::SlotBrush;
use crate::shape::engine::ShapedLayout;
use crate::shape::style::{Lists, LoweredStyle};

/// Shapes one paragraph, prefixing the control that forces its base direction.
///
/// The prefix goes in here rather than into the string a caller generated, because it is a
/// property of how this engine is driven and not of the document. It is therefore taken back off
/// again on the way out: the shaped result carries the caller's own string and the caller's own
/// map, and every offset read out of the layout afterwards — [line ranges](crate::shape::lines),
/// [cluster ranges](crate::shape::clusters) — has the prefix subtracted before it is reported.
///
/// So there is one byte space on this boundary rather than two. A shifted map beside offsets that
/// counted the prefix would be self-consistent and wrong in the same measure: mapping an offset out
/// and back would round-trip, while a caret placed from a cluster and a click resolved to one would
/// land a prefix apart from each other on the screen.
pub(crate) fn shape(
    key: ParagraphKey,
    content: &ParagraphContent<'_>,
    strut: StrutMetrics,
    controls: Controls,
    fonts: &mut parley::FontContext,
    scratch: &mut parley::LayoutContext<SlotBrush>,
) -> ShapedParagraph<ShapedLayout> {
    let prefix = controls.prefix(content.paragraph.direction);
    let mut text = String::with_capacity(prefix.len() + content.text.len());
    text.push_str(prefix);
    text.push_str(content.text);

    let mut layout = match uniform(content) {
        Some((style, brush)) => {
            uniform_layout(content, style, brush, &text, prefix, fonts, scratch)
        }
        None => ranged_layout(content, &text, prefix, fonts, scratch),
    };
    let widths = layout.calculate_content_widths();
    // A first break with no width constraint is what produces line metrics at all; every later
    // request re-breaks the same glyphs.
    layout.break_all_lines(None);
    let has_boxes = !content.boxes.is_empty();
    let wraps = content.runs.is_empty()
        || content
            .runs
            .iter()
            .any(|run| run.style.wrap_mode == zgui_text_style::WrapMode::Wrap);
    let engine = ShapedLayout {
        last: crate::shape::lines::read(&layout, has_boxes, prefix.len()),
        layout,
        prefix: prefix.len(),
        wraps,
    };
    ShapedParagraph::new(
        key,
        content.text.to_owned(),
        content.map.clone(),
        ContentWidths {
            min: CssPx(widths.min),
            max: CssPx(widths.max),
        },
        strut,
        content.boxes.iter().copied(),
        engine,
    )
}

/// Builds a paragraph whose runs all carry one style and one brush.
///
/// The ranged path resolves such a paragraph to that one style over the whole string: every run's
/// properties equal the defaults, so none of them splits the root span. Resolving the style once
/// skips the root style, the per-property pushes and the range splitting, and gives the same
/// layout.
fn uniform_layout(
    content: &ParagraphContent<'_>,
    style: &TextStyle,
    brush: Brush,
    text: &str,
    prefix: &str,
    fonts: &mut parley::FontContext,
    scratch: &mut parley::LayoutContext<SlotBrush>,
) -> parley::Layout<SlotBrush> {
    let lists = Lists::of(style);
    let mut builder = scratch.style_run_builder(fonts, text, content.scale, true);
    let index = builder.push_style(lists.text_style(style, brush, CssPx::ZERO));
    builder.push_style_run(index, 0..text.len());
    for geometry in content.boxes {
        builder.push_inline_box(inline_box(geometry, prefix.len()));
    }
    builder.build(text)
}

/// Builds a paragraph run by run over the first run's style.
fn ranged_layout(
    content: &ParagraphContent<'_>,
    text: &str,
    prefix: &str,
    fonts: &mut parley::FontContext,
    scratch: &mut parley::LayoutContext<SlotBrush>,
) -> parley::Layout<SlotBrush> {
    let mut builder = scratch.ranged_builder(fonts, text, content.scale, true);
    default_style(content).push_default(&mut builder);
    for run in content.runs {
        let range = run.text.start + prefix.len()..run.text.end + prefix.len();
        if range.is_empty() {
            continue;
        }
        LoweredStyle::of(&run.style, run.brush, CssPx::ZERO).push_over(&mut builder, range);
    }
    for geometry in content.boxes {
        builder.push_inline_box(inline_box(geometry, prefix.len()));
    }
    builder.build(text)
}

/// The one style and brush every run carries, if all runs carry the same.
///
/// A paragraph with no runs is measured in the initial style, as [`default_style`] measures it.
/// The ranged path skips runs with an empty range, so this skips them too.
fn uniform<'a>(content: &'a ParagraphContent<'_>) -> Option<(&'a TextStyle, Brush)> {
    let Some(first) = content.runs.first() else {
        return Some((initial(), zgui_scene::PaintSlot(0)));
    };
    content
        .runs
        .iter()
        .filter(|run| !run.text.is_empty())
        .all(|run| {
            run.brush == first.brush
                && (Arc::ptr_eq(&run.style, &first.style) || *run.style == *first.style)
        })
        .then_some((&*first.style, first.brush))
}

/// The initial style, built once.
fn initial() -> &'static TextStyle {
    static INITIAL: std::sync::OnceLock<TextStyle> = std::sync::OnceLock::new();
    INITIAL.get_or_init(TextStyle::initial)
}

/// The style the paragraph's own defaults come from.
///
/// The first run's, because a paragraph's runs are what it is made of and the first one is the
/// style anything outside every run — the directional prefix, and a paragraph with no runs at all
/// — is measured in. A paragraph with no runs falls back to the initial style, which is the same
/// answer an empty document would give.
pub(crate) fn default_style(content: &ParagraphContent<'_>) -> LoweredStyle {
    match content.runs.first() {
        Some(StyledRun { style, brush, .. }) => LoweredStyle::of(style, *brush, CssPx::ZERO),
        None => LoweredStyle::of(&TextStyle::initial(), zgui_scene::PaintSlot(0), CssPx::ZERO),
    }
}

/// One atomic inline, at the width and the declared height the caller measured it with.
fn inline_box(geometry: &InlineBoxGeometry, prefix: usize) -> parley::InlineBox {
    parley::InlineBox {
        id: geometry.id,
        kind: parley::InlineBoxKind::InFlow,
        index: geometry.offset + prefix,
        width: geometry.width.0,
        height: geometry.shaper_height().0,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zgui_geom::CssPx;
    use zgui_scene::PaintSlot;
    use zgui_text::{FontSource, InlineBoxGeometry, ParagraphContent, StyledRun, TextMap};
    use zgui_text_style::{
        Direction, FamilyName, FontFamilyList, GenericFamily, LineHeight, ParagraphStyle, TextStyle,
    };

    use super::{ranged_layout, uniform, uniform_layout};
    use crate::direction::Controls;
    use crate::shape::brush::SlotBrush;
    use crate::{FontSystem, FontSystemOptions};

    /// The shipped Latin and Arabic faces, and nothing the machine has.
    fn fonts() -> Arc<FontSystem> {
        let fonts = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
        for file in ["NotoSans-Regular.ttf", "NotoSansArabic-Regular.ttf"] {
            let path = format!("{}/tests/fonts/{file}", env!("CARGO_MANIFEST_DIR"));
            let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> =
                Arc::new(std::fs::read(&path).expect("a shipped face"));
            fonts.register(bytes, None).expect("a readable face");
        }
        fonts
    }

    /// Everything the layout holds that a reader of it can see, as text.
    fn fingerprint(mut layout: parley::Layout<SlotBrush>) -> Vec<String> {
        let widths = layout.calculate_content_widths();
        layout.break_all_lines(Some(60.0));
        let mut out = vec![
            format!("{:?} {:?}", widths.min, widths.max),
            format!("{:?}", layout.styles()),
        ];
        for line in layout.lines() {
            out.push(format!("{:?} {:?}", line.metrics(), line.text_range()));
            for item in line.items() {
                match item {
                    parley::PositionedLayoutItem::GlyphRun(run) => {
                        let text = run.run();
                        out.push(format!(
                            "run {} {} {} {:?} {:?} {:?} {:?}",
                            text.font().data.id(),
                            text.font().index,
                            text.font_size(),
                            text.synthesis(),
                            text.text_range(),
                            text.is_rtl(),
                            run.style(),
                        ));
                        out.extend(run.positioned_glyphs().map(|glyph| format!("{glyph:?}")));
                    }
                    parley::PositionedLayoutItem::InlineBox(inline) => {
                        out.push(format!("{inline:?}"));
                    }
                }
            }
        }
        out
    }

    /// A paragraph of one style whose runs both builders shape to the same layout.
    #[test]
    fn a_single_style_paragraph_lays_out_the_same_on_both_paths() {
        let fonts = fonts();
        let mut context = fonts.font_context();
        let mut scratch = parley::LayoutContext::<SlotBrush>::new();
        let named = TextStyle {
            family: FontFamilyList::from_iter([
                FamilyName::Named(zgui_interned::Ident::new("Noto Sans Arabic")),
                FamilyName::Named(zgui_interned::Ident::new("Noto Sans")),
            ]),
            size: CssPx(13.5),
            letter_spacing: zgui_text_style::LengthPercent {
                length: CssPx(0.25),
                percent: 0.0,
            },
            line_height: LineHeight::Number(1.4),
            ..TextStyle::initial()
        };
        let generic = TextStyle {
            family: FontFamilyList::from_iter([FamilyName::Generic(GenericFamily::Monospace)]),
            weight: 700.0,
            ..TextStyle::initial()
        };
        let texts = [
            "",
            "-0.25",
            "6.5",
            "1e-3 \u{1F600} 42",
            "report.pdf ملف تقرير سنوي",
            "the quick brown fox jumps over the lazy dog",
            "a\nb",
        ];
        for (style, direction, scale) in [
            (TextStyle::initial(), Direction::LeftToRight, 1.0),
            (named, Direction::RightToLeft, 1.5),
            (generic, Direction::LeftToRight, 2.0),
        ] {
            let style = Arc::new(style);
            for text in texts {
                let split = text.len() / 2;
                let split = (0..=split)
                    .rev()
                    .find(|at| text.is_char_boundary(*at))
                    .unwrap_or(0);
                let mut map = TextMap::new();
                map.push(0..text.len(), 0, 0);
                // Two runs of one style, and an empty run between them, which is still uniform.
                let runs = [
                    StyledRun {
                        text: 0..split,
                        style: Arc::clone(&style),
                        brush: PaintSlot(3),
                    },
                    StyledRun {
                        text: split..split,
                        style: Arc::new(TextStyle::initial()),
                        brush: PaintSlot(9),
                    },
                    StyledRun {
                        text: split..text.len(),
                        style: Arc::new((*style).clone()),
                        brush: PaintSlot(3),
                    },
                ];
                let boxes = [InlineBoxGeometry {
                    id: 7,
                    offset: split,
                    width: CssPx(12.0),
                    height: CssPx(9.0),
                    ascent: CssPx(9.0),
                    shift: CssPx::ZERO,
                }];
                let paragraph = ParagraphStyle {
                    direction,
                    ..ParagraphStyle::initial()
                };
                for (runs, boxes) in [
                    (&runs[..], &boxes[..0]),
                    (&runs[..], &boxes[..]),
                    (&[][..], &[][..]),
                ] {
                    let content = ParagraphContent {
                        text,
                        map: &map,
                        runs,
                        boxes,
                        paragraph: &paragraph,
                        scale,
                    };
                    let prefix = Controls::Mark.prefix(direction);
                    let full = format!("{prefix}{text}");
                    let (style, brush) = uniform(&content).expect("one style throughout");
                    let fast = uniform_layout(
                        &content,
                        style,
                        brush,
                        &full,
                        prefix,
                        &mut context,
                        &mut scratch,
                    );
                    let ranged = ranged_layout(&content, &full, prefix, &mut context, &mut scratch);
                    assert_eq!(
                        fingerprint(fast),
                        fingerprint(ranged),
                        "{text:?} at {scale}"
                    );
                }
            }
        }
    }

    /// Runs of different styles or brushes take the ranged path.
    #[test]
    fn mixed_runs_are_not_uniform() {
        let text = "ab";
        let mut map = TextMap::new();
        map.push(0..text.len(), 0, 0);
        let style = Arc::new(TextStyle::initial());
        let bold = Arc::new(TextStyle {
            weight: 700.0,
            ..TextStyle::initial()
        });
        let paragraph = ParagraphStyle::initial();
        for (second, brush) in [(&bold, PaintSlot(0)), (&style, PaintSlot(1))] {
            let runs = [
                StyledRun {
                    text: 0..1,
                    style: Arc::clone(&style),
                    brush: PaintSlot(0),
                },
                StyledRun {
                    text: 1..2,
                    style: Arc::clone(second),
                    brush,
                },
            ];
            let content = ParagraphContent {
                text,
                map: &map,
                runs: &runs,
                boxes: &[],
                paragraph: &paragraph,
                scale: 1.0,
            };
            assert!(uniform(&content).is_none());
        }
    }
}
