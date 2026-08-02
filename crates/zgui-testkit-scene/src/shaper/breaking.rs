//! Greedy line breaking over the clusters, at spaces only.

use zgui_text::BreakRequest;
use zgui_text_style::WrapMode;

use crate::shaper::cluster::Cluster;

/// Breaks `clusters` into lines for `request`, as half-open cluster ranges.
///
/// The rule is the simplest one that is still a line breaker: take the last break opportunity that
/// fits, and if none does, overflow the line rather than splitting a word. A paragraph that cannot
/// wrap at all — `text-wrap-mode: nowrap` — is one line however wide it is.
///
/// Atomic inlines occupy the line beside the text, so their widths are added at the offsets they
/// sit at; a line holding a wide image therefore breaks earlier than the same text alone, which is
/// what makes a break-key test over inline boxes mean anything.
pub fn into_lines(clusters: &[Cluster], request: &BreakRequest<'_>) -> Vec<(usize, usize)> {
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
        let boxes: f32 = request
            .boxes
            .iter()
            .filter(|geometry| geometry.offset == cluster.offset)
            .map(|geometry| geometry.width.0)
            .sum();
        if cluster.breakable && index > start {
            last_opportunity = Some(index);
        }
        let wanted = cluster.advance.0 + boxes;
        // A line with no break opportunity on it overflows rather than splitting a word: there is
        // nowhere legal to break, and inventing a break inside a word is a different feature.
        if let Some(split) =
            last_opportunity.filter(|_| advance + wanted > limit.0 && index > start)
        {
            lines.push((start, split));
            start = split;
            advance = clusters[start..=index]
                .iter()
                .map(|held| held.advance.0)
                .sum::<f32>()
                + boxes;
            last_opportunity = None;
        } else {
            advance += wanted;
        }
    }
    lines.push((start, clusters.len()));
    lines
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zgui_geom::CssPx;
    use zgui_scene::PaintSlot;
    use zgui_text::{BreakRequest, StyledRun};
    use zgui_text_style::{ParagraphStyle, TextStyle, WrapMode};

    use crate::shaper::cluster::{Cluster, shape};

    use super::into_lines;

    /// The clusters of `text`, in the initial style.
    fn clusters(text: &str) -> Vec<Cluster> {
        let map = zgui_text::TextMap::new();
        let runs = [StyledRun {
            text: 0..text.len(),
            style: Arc::new(TextStyle::initial()),
            brush: PaintSlot(0),
        }];
        let paragraph = ParagraphStyle::initial();
        shape(&zgui_text::ParagraphContent {
            text,
            map: &map,
            runs: &runs,
            boxes: &[],
            paragraph: &paragraph,
            scale: 1.0,
        })
    }

    /// A request at one width, in one style.
    fn request<'a>(
        runs: &'a [StyledRun],
        paragraph: &'a ParagraphStyle,
        width: Option<CssPx>,
    ) -> BreakRequest<'a> {
        BreakRequest {
            runs,
            boxes: &[],
            paragraph,
            max_advance: width,
            indent_basis: width,
            bands: zgui_text::LineBands::NONE,
            probe: false,
        }
    }

    #[test]
    fn a_break_is_taken_at_the_last_space_that_fits() {
        // Eight pixels a cluster: "aaa bbb" is 56 wide, so 40 fits "aaa " and breaks before "bbb".
        let text = "aaa bbb";
        let runs = [StyledRun {
            text: 0..text.len(),
            style: Arc::new(TextStyle::initial()),
            brush: PaintSlot(0),
        }];
        let paragraph = ParagraphStyle::initial();
        let lines = into_lines(
            &clusters(text),
            &request(&runs, &paragraph, Some(CssPx(40.0))),
        );
        assert_eq!(lines, vec![(0, 3), (3, 7)]);
    }

    #[test]
    fn a_word_wider_than_the_line_overflows_rather_than_splitting() {
        let text = "aaaaaaaa";
        let runs = [StyledRun {
            text: 0..text.len(),
            style: Arc::new(TextStyle::initial()),
            brush: PaintSlot(0),
        }];
        let paragraph = ParagraphStyle::initial();
        let lines = into_lines(
            &clusters(text),
            &request(&runs, &paragraph, Some(CssPx(16.0))),
        );
        assert_eq!(lines, vec![(0, 8)]);
    }

    #[test]
    fn nowrap_is_one_line_however_narrow_the_width() {
        let text = "aaa bbb";
        let mut style = TextStyle::initial();
        style.wrap_mode = WrapMode::NoWrap;
        let runs = [StyledRun {
            text: 0..text.len(),
            style: Arc::new(style),
            brush: PaintSlot(0),
        }];
        let paragraph = ParagraphStyle::initial();
        let lines = into_lines(
            &clusters(text),
            &request(&runs, &paragraph, Some(CssPx(8.0))),
        );
        assert_eq!(lines, vec![(0, 7)]);
    }

    #[test]
    fn no_width_at_all_is_one_line() {
        let text = "aaa bbb";
        let runs = [StyledRun {
            text: 0..text.len(),
            style: Arc::new(TextStyle::initial()),
            brush: PaintSlot(0),
        }];
        let paragraph = ParagraphStyle::initial();
        let lines = into_lines(&clusters(text), &request(&runs, &paragraph, None));
        assert_eq!(lines, vec![(0, 7)]);
    }
}
