//! A deterministic shaper with no font files behind it.
//!
//! Every character is one cluster of a fixed width, so the contract can be exercised — and the
//! counters it is supposed to move can be asserted on — without a font engine, a font file or a
//! machine-dependent answer anywhere.

use smallvec::SmallVec;
use zgui_geom::{Css, CssPx, Point, Size};
use zgui_text::{
    BreakRequest, BrokenParagraph, ContentWidths, InlineBoxGeometry, InlineBoxPlacement,
    LineGeometry, ParagraphContent, ParagraphKey, ParagraphShaper, ShapedGlyph, ShapedParagraph,
    ShapedRun, StrutMetrics, TextGeometry,
};
use zgui_text_style::{TextAlign, TextStyle, WrapMode};

/// Width of one character, as a fraction of the font size.
pub(crate) const ADVANCE_RATIO: f32 = 0.5;
/// Ascent, as a fraction of the font size.
pub(crate) const ASCENT_RATIO: f32 = 0.8;
/// Descent, as a fraction of the font size.
pub(crate) const DESCENT_RATIO: f32 = 0.2;

/// One shaped cluster.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Cluster {
    /// Byte offset of the character in the generated string.
    pub(crate) offset: usize,
    /// Its advance.
    pub(crate) advance: CssPx,
    /// Whether a soft break may be taken before it.
    pub(crate) breakable: bool,
}

/// What this shaper keeps between shaping and breaking.
#[derive(Clone, Debug, Default)]
pub(crate) struct MonoLayout {
    /// The clusters, in logical order.
    pub(crate) clusters: Vec<Cluster>,
    /// The last break's line ranges, as cluster indices.
    pub(crate) lines: Vec<(usize, usize)>,
}

/// The shaper.
#[derive(Debug, Default)]
pub(crate) struct MonoShaper {
    /// How many times a paragraph was shaped, counted here as well so a test can check the
    /// framework's own counter against the shaper's own view of what it did.
    pub(crate) shapes: u32,
    /// How many breaking passes actually ran.
    pub(crate) breaks: u32,
}

impl ParagraphShaper for MonoShaper {
    type Engine = MonoLayout;

    fn shape(&mut self, content: &ParagraphContent<'_>) -> ShapedParagraph<Self::Engine> {
        self.shapes += 1;
        let size = content
            .runs
            .first()
            .map_or(CssPx(16.0), |run| run.style.size);

        let mut clusters = Vec::new();
        for (offset, character) in content.text.char_indices() {
            clusters.push(Cluster {
                offset,
                advance: CssPx(size.0 * ADVANCE_RATIO),
                breakable: character == ' ',
            });
        }

        let widths = ContentWidths {
            min: longest_word(content.text, CssPx(size.0 * ADVANCE_RATIO)),
            max: CssPx(clusters.len() as f32 * size.0 * ADVANCE_RATIO),
        };
        ShapedParagraph::new(
            ParagraphKey::of(content),
            content.text.to_owned(),
            content.map.clone(),
            widths,
            self.strut(content.runs.first().map_or(&FALLBACK, |run| &run.style)),
            content.boxes.iter().copied(),
            MonoLayout {
                clusters,
                lines: Vec::new(),
            },
        )
    }

    fn break_lines(
        &mut self,
        shaped: &mut ShapedParagraph<Self::Engine>,
        request: &BreakRequest<'_>,
    ) -> BrokenParagraph {
        match shaped.plan_break(request) {
            zgui_text::Plan::Reflected => return assemble(shaped, request),
            zgui_text::Plan::Recalled(broken) => return broken.clone(),
            zgui_text::Plan::Owed => {}
        }
        self.breaks += 1;
        let lines = break_into_lines(&shaped.engine.clusters, request);
        shaped.engine.lines = lines;
        let broken = assemble(shaped, request);
        shaped.remember(request.key(), broken.clone());
        broken
    }

    /// One glyph per cluster of the line, laid end to end from the line box's left edge.
    fn visit_line(
        &self,
        shaped: &ShapedParagraph<Self::Engine>,
        line: u16,
        visit: &mut dyn FnMut(ShapedRun<'_>),
    ) {
        let Some((start, end)) = shaped.engine.lines.get(line as usize).copied() else {
            return;
        };
        let baseline = shaped.strut().ascent().0;
        let mut pen = 0.0;
        let mut glyphs = Vec::new();
        for (index, cluster) in shaped.engine.clusters[start..end].iter().enumerate() {
            glyphs.push(ShapedGlyph {
                glyph: (start + index) as u16,
                x: pen,
                y: baseline,
            });
            pen += cluster.advance.0;
        }
        if glyphs.is_empty() {
            return;
        }
        visit(ShapedRun {
            face: zgui_text::FaceId(0),
            size: shaped.strut().font_size.0,
            synthetic_bold: 0.0,
            synthetic_slant: 0.0,
            has_color: false,
            brush: zgui_scene::PaintSlot(0),
            glyphs: &glyphs,
        });
    }

    /// One cluster per character of the line, laid end to end from the line box's start edge.
    fn visit_clusters(
        &self,
        shaped: &ShapedParagraph<Self::Engine>,
        line: u16,
        visit: &mut dyn FnMut(zgui_text::ClusterRun<'_>),
    ) {
        let Some((start, end)) = shaped.engine.lines.get(line as usize).copied() else {
            return;
        };
        let mut pen = 0.0;
        let mut clusters = Vec::new();
        for (index, cluster) in shaped.engine.clusters[start..end].iter().enumerate() {
            let next = shaped
                .engine
                .clusters
                .get(start + index + 1)
                .map_or(shaped.text().len(), |next| next.offset);
            clusters.push(zgui_text::ClusterGeometry {
                text: cluster.offset..next,
                offset: CssPx(pen),
                advance: cluster.advance,
            });
            pen += cluster.advance.0;
        }
        if clusters.is_empty() {
            return;
        }
        visit(zgui_text::ClusterRun {
            direction: zgui_text::TextDirection::LeftToRight,
            start: CssPx(0.0),
            clusters: &clusters,
        });
    }

    fn strut(&mut self, style: &TextStyle) -> StrutMetrics {
        let ascent = CssPx(style.size.0 * ASCENT_RATIO);
        let descent = CssPx(style.size.0 * DESCENT_RATIO);
        StrutMetrics {
            font_ascent: ascent,
            font_descent: descent,
            line_height: style
                .line_height
                .resolve(style.size, CssPx(ascent.0 + descent.0)),
            x_height: CssPx(style.size.0 * 0.5),
            font_size: style.size,
        }
    }
}

/// The style used when a paragraph holds no text runs at all.
static FALLBACK: std::sync::LazyLock<TextStyle> = std::sync::LazyLock::new(TextStyle::initial);

/// The widest run of non-space characters, which is the narrowest the paragraph can be.
fn longest_word(text: &str, advance: CssPx) -> CssPx {
    let longest = text
        .split(' ')
        .map(|word| word.chars().count())
        .max()
        .unwrap_or(0);
    CssPx(longest as f32 * advance.0)
}

/// Greedy line breaking: take a break at the last opportunity that still fits.
fn break_into_lines(clusters: &[Cluster], request: &BreakRequest<'_>) -> Vec<(usize, usize)> {
    let wrapping = request
        .runs
        .first()
        .is_none_or(|run| run.style.wrap_mode == WrapMode::Wrap);
    let Some(limit) = request.max_advance.filter(|_| wrapping) else {
        return vec![(0, clusters.len())];
    };

    let mut lines = Vec::new();
    let mut start = 0;
    let mut advance = request.indent().0;
    let mut last_opportunity = None;
    for (index, cluster) in clusters.iter().enumerate() {
        if cluster.breakable && index > start {
            last_opportunity = Some(index);
        }
        if advance + cluster.advance.0 > limit.0 && index > start {
            let split = last_opportunity.unwrap_or(index);
            lines.push((start, split));
            start = split;
            advance = clusters[start..=index].iter().map(|c| c.advance.0).sum();
            last_opportunity = None;
        } else {
            advance += cluster.advance.0;
        }
    }
    lines.push((start, clusters.len()));
    lines
}

/// Builds the geometry the caller reads, from the line ranges the last break produced.
fn assemble(shaped: &ShapedParagraph<MonoLayout>, request: &BreakRequest<'_>) -> BrokenParagraph {
    let strut = shaped.strut();
    let clusters = &shaped.engine.clusters;
    let mut lines = Vec::new();
    let mut boxes: SmallVec<[InlineBoxPlacement; 2]> = SmallVec::new();
    let mut top = CssPx::ZERO;
    let mut widest: f32 = 0.0;

    for (index, (start, end)) in shaped.engine.lines.iter().copied().enumerate() {
        let on_line: Vec<&InlineBoxGeometry> = shaped
            .boxes()
            .iter()
            .filter(|geometry| {
                clusters
                    .get(start)
                    .is_none_or(|first| geometry.offset >= first.offset)
                    && clusters
                        .get(end)
                        .is_none_or(|past| geometry.offset < past.offset)
            })
            .collect();

        let ascent = on_line.iter().fold(strut.ascent(), |tallest, geometry| {
            CssPx(tallest.0.max(geometry.shaper_height().0))
        });
        let descent = on_line.iter().fold(strut.descent(), |deepest, geometry| {
            CssPx(deepest.0.max(geometry.below_baseline().0))
        });
        let text_advance: f32 = clusters[start..end].iter().map(|c| c.advance.0).sum();
        let box_advance: f32 = on_line.iter().map(|geometry| geometry.width.0).sum();
        let width = text_advance + box_advance;
        let indent = if index == 0 { request.indent().0 } else { 0.0 };
        let offset = align_offset(request, CssPx(width + indent)).0 + indent;

        let baseline = CssPx(top.0 + ascent.0);
        for geometry in on_line {
            boxes.push(InlineBoxPlacement {
                id: geometry.id,
                origin: Point::new(
                    CssPx(offset),
                    CssPx(baseline.0 - geometry.shaper_height().0),
                ),
                line: index,
            });
        }

        widest = widest.max(width + indent);
        lines.push(LineGeometry {
            text: clusters.get(start).map_or(0, |c| c.offset)
                ..clusters.get(end).map_or(shaped.text().len(), |c| c.offset),
            top,
            baseline,
            height: CssPx(ascent.0 + descent.0),
            width: CssPx(width),
            offset: CssPx(offset),
        });
        top = CssPx(top.0 + ascent.0 + descent.0);
    }

    BrokenParagraph {
        geometry: std::sync::Arc::new(TextGeometry {
            size: Size::<CssPx, Css>::new(CssPx(widest), top),
            lines,
            is_rtl: false,
        }),
        boxes,
    }
}

/// How far one line is pushed in by alignment.
fn align_offset(request: &BreakRequest<'_>, width: CssPx) -> CssPx {
    let Some(limit) = request.max_advance else {
        return CssPx::ZERO;
    };
    let free = (limit.0 - width.0).max(0.0);
    match request.paragraph.align {
        TextAlign::Center => CssPx(free / 2.0),
        TextAlign::End | TextAlign::Right => CssPx(free),
        _ => CssPx::ZERO,
    }
}
