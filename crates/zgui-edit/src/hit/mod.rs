//! Turning a point into a text offset, and an offset back into a caret.
//!
//! Both directions have to agree with the layout the shaper actually produced, and neither can be
//! computed from the string: which bytes are drawn where is what shaping decided, and on a
//! bidirectional line the drawn order is not the byte order at all.
//!
//! # The two answers one position has
//!
//! At a boundary between two directions, one *offset* is drawn in two places — after the last
//! letter of the right-to-left run, and before the first letter of the left-to-right one — and one
//! *place* on the screen means two offsets, for the same reason seen from the other side. Neither
//! is a defect to be resolved: the two readings are both correct and an editor has to keep them
//! apart, which is what [`Affinity`] is for. A hit reports which of the
//! two it found, and asking for the caret of an offset requires saying which one is meant.

pub mod band;
pub mod caret;
pub mod line;

use zgui_geom::{Css, CssPx, Point};
use zgui_text::{
    BrokenParagraph, LineGeometry, ParagraphShaper, ShapedParagraph, SourcePos, TextMap,
};

pub use crate::hit::band::Band;
pub use crate::hit::caret::Caret;
pub use crate::hit::line::{Line, Run};

use crate::select::Affinity;

/// Where a point landed in the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The offset in the *generated* string the shaper was handed.
    pub offset: usize,
    /// Which of the two carets that offset has was hit.
    pub affinity: Affinity,
    /// The line it landed on.
    pub line: usize,
}

/// One paragraph's lines and clusters, ready to be asked where things are.
///
/// Built from a shaped and broken paragraph and held for as long as that break is current. Every
/// coordinate it takes and returns is relative to the paragraph's own top-left corner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineMap {
    /// The lines, in order.
    lines: Vec<Line>,
}

impl LineMap {
    /// Reads the lines and clusters out of a shaped paragraph.
    ///
    /// The break has to be the current one: the geometry comes from `broken` and the clusters from
    /// the shaper's own view of the same break, so a map built from a stale [`BrokenParagraph`]
    /// would place clusters on lines they are not on.
    pub fn of<S: ParagraphShaper>(
        shaper: &S,
        shaped: &ShapedParagraph<S::Engine>,
        broken: &BrokenParagraph,
    ) -> Self {
        let mut lines = Vec::with_capacity(broken.geometry.lines.len());
        for (index, geometry) in broken.geometry.lines.iter().enumerate() {
            let mut runs = Vec::new();
            shaper.visit_clusters(shaped, index as u16, &mut |run| runs.push(Run::of(&run)));
            lines.push(Line {
                geometry: geometry.clone(),
                runs,
            });
        }
        Self { lines }
    }

    /// Reads the lines and clusters from geometry a caller already has.
    ///
    /// The same map as [`LineMap::of`], for a caller holding line boxes that were computed once
    /// and kept — a laid-out document holds exactly that, and breaking its paragraphs again to
    /// rebuild them would be a second opinion about where the lines are. `clusters` is asked for
    /// one line at a time, by index, in the same coordinates the geometry is measured in.
    ///
    /// ```
    /// use zgui_edit::hit::LineMap;
    /// use zgui_edit::select::Affinity;
    /// use zgui_geom::CssPx;
    /// use zgui_text::{ClusterGeometry, ClusterRun, LineGeometry, TextDirection};
    ///
    /// let lines = [LineGeometry {
    ///     text: 0..2,
    ///     top: CssPx(0.0),
    ///     baseline: CssPx(12.0),
    ///     height: CssPx(16.0),
    ///     width: CssPx(16.0),
    ///     offset: CssPx(0.0),
    /// }];
    /// let clusters = [
    ///     ClusterGeometry { text: 0..1, offset: CssPx(0.0), advance: CssPx(8.0) },
    ///     ClusterGeometry { text: 1..2, offset: CssPx(8.0), advance: CssPx(8.0) },
    /// ];
    /// let map = LineMap::of_lines(&lines, |_line, visit| {
    ///     visit(ClusterRun {
    ///         direction: TextDirection::LeftToRight,
    ///         start: CssPx(0.0),
    ///         clusters: &clusters,
    ///     });
    /// });
    /// let caret = map.caret(1, Affinity::Downstream).expect("the second cluster's leading edge");
    /// assert_eq!(caret.origin.x, CssPx(8.0));
    /// ```
    pub fn of_lines<F>(lines: &[LineGeometry], mut clusters: F) -> Self
    where
        F: FnMut(u16, &mut dyn FnMut(zgui_text::ClusterRun<'_>)),
    {
        let mut held = Vec::with_capacity(lines.len());
        for (index, geometry) in lines.iter().enumerate() {
            let mut runs = Vec::new();
            clusters(index as u16, &mut |run| runs.push(Run::of(&run)));
            held.push(Line {
                geometry: geometry.clone(),
                runs,
            });
        }
        Self { lines: held }
    }

    /// The lines, in order.
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// Whether nothing was laid out.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The offset a point in the paragraph landed on.
    ///
    /// A point above the first line lands on the first, one below the last lands on the last, and
    /// one beyond a line's end lands at that line's end — a click in the margin belongs to the line
    /// beside it, which is what makes dragging a selection past the text work.
    pub fn hit(&self, point: Point<CssPx, Css>) -> Option<Hit> {
        let index = self.line_at_y(point.y.0)?;
        let line = &self.lines[index];
        let x = point.x.0 - line.geometry.offset.0;
        let Some(run) = line.run_at(x) else {
            return Some(Hit {
                offset: line.geometry.text.start,
                affinity: Affinity::Downstream,
                line: index,
            });
        };
        let Some((cluster, into)) = run.cluster_at(x - run.start.0) else {
            return Some(Hit {
                offset: line.geometry.text.start,
                affinity: Affinity::Downstream,
                line: index,
            });
        };
        let half = cluster.advance.0 / 2.0;
        let leading = if run.is_rtl() {
            into > half
        } else {
            into < half
        };
        Some(Hit {
            offset: if leading {
                cluster.text.start
            } else {
                cluster.text.end
            },
            affinity: if leading {
                Affinity::Downstream
            } else {
                Affinity::Upstream
            },
            line: index,
        })
    }

    /// Where the caret for an offset is drawn.
    ///
    /// The affinity decides between the two places an offset at a direction boundary or at a soft
    /// line break has. An offset with no cluster claiming it on either reading — one inside a
    /// ligature, or past the text — answers with the nearest edge that does, and a line with
    /// nothing on it answers with its own start edge, which is where the caret of an empty field
    /// goes.
    pub fn caret(&self, offset: usize, affinity: Affinity) -> Option<Caret> {
        let (index, x) = self
            .edge(offset, affinity)
            .or_else(|| self.empty_line_edge(offset))
            .or_else(|| self.edge(offset, affinity.flipped()))
            .or_else(|| self.nearest_edge(offset))
            .or_else(|| self.blank_edge(offset))?;
        let line = &self.lines[index];
        Some(Caret {
            origin: Point::new(
                CssPx(line.geometry.offset.0 + x),
                CssPx(line.geometry.top.0),
            ),
            height: line.geometry.height,
            line: index,
        })
    }

    /// The rectangles a selected range is painted as.
    ///
    /// One band per contiguous stretch of one line, so a range that a direction boundary splits in
    /// two is two rectangles rather than one covering the unselected text between them. An empty
    /// range paints nothing at all: a caret is not a selection, and a zero-width band would be a
    /// second caret in a colour nobody chose.
    ///
    /// The bands are in line order and, within a line, left to right.
    ///
    /// ```
    /// use zgui_edit::hit::LineMap;
    /// use zgui_geom::CssPx;
    /// use zgui_text::{ClusterGeometry, ClusterRun, LineGeometry, TextDirection};
    ///
    /// let lines = [LineGeometry {
    ///     text: 0..3,
    ///     top: CssPx(0.0),
    ///     baseline: CssPx(12.0),
    ///     height: CssPx(16.0),
    ///     width: CssPx(24.0),
    ///     offset: CssPx(4.0),
    /// }];
    /// let clusters: Vec<ClusterGeometry> = (0..3)
    ///     .map(|index| ClusterGeometry {
    ///         text: index..index + 1,
    ///         offset: CssPx(index as f32 * 8.0),
    ///         advance: CssPx(8.0),
    ///     })
    ///     .collect();
    /// let map = LineMap::of_lines(&lines, |_line, visit| {
    ///     visit(ClusterRun {
    ///         direction: TextDirection::LeftToRight,
    ///         start: CssPx(0.0),
    ///         clusters: &clusters,
    ///     });
    /// });
    ///
    /// let bands = map.highlight(1..3);
    /// assert_eq!(bands.len(), 1);
    /// assert_eq!(bands[0].origin.x, CssPx(12.0), "the line's own inset plus one advance");
    /// assert_eq!(bands[0].size.width, CssPx(16.0));
    /// assert!(map.highlight(2..2).is_empty(), "a caret is not a selection");
    /// ```
    pub fn highlight(&self, range: core::ops::Range<usize>) -> Vec<Band> {
        if range.is_empty() {
            return Vec::new();
        }
        let mut bands = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            let mut intervals = Vec::new();
            for run in &line.runs {
                for cluster in &run.clusters {
                    if cluster.text.start >= range.end || cluster.text.end <= range.start {
                        continue;
                    }
                    let start = run.start.0 + cluster.offset.0;
                    intervals.push((start, start + cluster.advance.0));
                }
            }
            for (start, end) in band::merge(intervals) {
                bands.push(Band {
                    line: index,
                    origin: Point::new(
                        CssPx(line.geometry.offset.0 + start),
                        CssPx(line.geometry.top.0),
                    ),
                    size: zgui_geom::Size::new(CssPx(end - start), line.geometry.height),
                });
            }
        }
        bands
    }

    /// The source position a hit corresponds to.
    ///
    /// Every offset a shaper reports indexes the generated string — white space collapsed, a
    /// direction control prefixed — so a hit is only usable after being mapped back, and a hit on
    /// a cluster the source never held snaps to the nearest one that it did.
    pub fn to_source(&self, hit: Hit, map: &TextMap) -> Option<SourcePos> {
        map.to_source(hit.offset)
            .or_else(|| map.to_source_snapped(hit.offset))
    }

    /// The line a vertical coordinate falls on.
    fn line_at_y(&self, y: f32) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }
        for (index, line) in self.lines.iter().enumerate() {
            if y < line.geometry.top.0 + line.geometry.height.0 {
                return Some(index);
            }
        }
        Some(self.lines.len() - 1)
    }

    /// The line and the x of one reading of an offset.
    fn edge(&self, offset: usize, affinity: Affinity) -> Option<(usize, f32)> {
        for (index, line) in self.lines.iter().enumerate() {
            for run in &line.runs {
                for cluster in &run.clusters {
                    let matched = match affinity {
                        Affinity::Downstream => cluster.text.start == offset,
                        Affinity::Upstream => cluster.text.end == offset,
                    };
                    if matched {
                        return Some((index, run.start.0 + within(run, cluster, affinity)));
                    }
                }
            }
        }
        None
    }

    /// The start edge of a line with nothing on it that begins at the offset.
    ///
    /// The line after a hard break holds no clusters until something is typed onto it, so neither
    /// reading of the offset finds an edge there — and falling through to the flipped reading
    /// instead finds the trailing edge of the break character, which is the end of the line
    /// *above*. That is the caret that stays on the old line after <kbd>Enter</kbd>. Consulted
    /// after the asked-for reading so a soft-wrap boundary, where both lines hold clusters, keeps
    /// its two distinct answers.
    fn empty_line_edge(&self, offset: usize) -> Option<(usize, f32)> {
        self.lines
            .iter()
            .position(|line| line.geometry.text.is_empty() && line.geometry.text.start == offset)
            .map(|index| (index, 0.0))
    }

    /// The start edge of the line an offset falls on, for a paragraph holding no clusters at all.
    ///
    /// An empty field is the ordinary state of one nobody has typed into yet, and it has one line
    /// with nothing on it: no cluster claims offset zero, and without this the caret of every empty
    /// field is simply absent. The x is zero because it is measured from the line box's own start
    /// edge, which is where the first character would be drawn whichever way the paragraph runs.
    fn blank_edge(&self, offset: usize) -> Option<(usize, f32)> {
        let index = self
            .lines
            .iter()
            .position(|line| line.geometry.text.contains(&offset))
            .unwrap_or(self.lines.len().checked_sub(1)?);
        Some((index, 0.0))
    }

    /// The nearest edge to an offset no cluster claims on either reading.
    fn nearest_edge(&self, offset: usize) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32, usize)> = None;
        for (index, line) in self.lines.iter().enumerate() {
            for run in &line.runs {
                for cluster in &run.clusters {
                    for (edge, affinity) in [
                        (cluster.text.start, Affinity::Downstream),
                        (cluster.text.end, Affinity::Upstream),
                    ] {
                        let gap = edge.abs_diff(offset);
                        if best.is_none_or(|(_, _, held)| gap < held) {
                            best = Some((index, run.start.0 + within(run, cluster, affinity), gap));
                        }
                    }
                }
            }
        }
        best.map(|(index, x, _)| (index, x))
    }
}

/// Where one edge of a cluster sits inside its run.
///
/// The leading edge of a right-to-left cluster is its right-hand side, which is the whole of what
/// makes a caret at a direction boundary land where the text it belongs to actually is.
fn within(run: &Run, cluster: &zgui_text::ClusterGeometry, affinity: Affinity) -> f32 {
    let leading = matches!(affinity, Affinity::Downstream);
    if run.is_rtl() == leading {
        cluster.offset.0 + cluster.advance.0
    } else {
        cluster.offset.0
    }
}
