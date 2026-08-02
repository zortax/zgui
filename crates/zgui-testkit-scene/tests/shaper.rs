//! The deterministic shaper: the protocol it honours, the metrics it reports, and the font files it
//! does not open.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use zgui_geom::CssPx;
use zgui_scene::PaintSlot;
use zgui_testkit_scene::{MonoLayout, MonoShaper};
use zgui_text::{
    BreakRequest, BrokenParagraph, InlineBoxGeometry, ParagraphCache, ParagraphContent,
    ParagraphShaper, StyledRun, TextMap, lay_out,
};
use zgui_text_style::{Direction, ParagraphStyle, TextAlign, TextStyle};

/// One paragraph, ready to be laid out at whatever width a test proposes.
struct Paragraph {
    /// The generated string.
    text: String,
    /// The way back to the source.
    map: TextMap,
    /// The runs.
    runs: Vec<StyledRun>,
    /// The atomic inlines.
    boxes: Vec<InlineBoxGeometry>,
    /// The paragraph's own properties.
    paragraph: ParagraphStyle,
    /// The shaper.
    shaper: MonoShaper,
    /// The shaped results held across calls.
    cache: ParagraphCache<MonoLayout>,
}

impl Paragraph {
    /// One paragraph of plain text in one style.
    fn new(text: &str, style: TextStyle) -> Self {
        let mut map = TextMap::new();
        map.push(0..text.len(), 0, 0);
        Self {
            runs: vec![StyledRun {
                text: 0..text.len(),
                style: Arc::new(style),
                brush: PaintSlot(0),
            }],
            text: text.to_owned(),
            map,
            boxes: Vec::new(),
            paragraph: ParagraphStyle::initial(),
            shaper: MonoShaper::new(),
            cache: ParagraphCache::new(),
        }
    }

    /// Lays the paragraph out at one width, shaping only if this is new content.
    fn run(&mut self, width: Option<CssPx>) -> BrokenParagraph {
        let content = ParagraphContent {
            text: &self.text,
            map: &self.map,
            runs: &self.runs,
            boxes: &self.boxes,
            paragraph: &self.paragraph,
            scale: 1.0,
        };
        let request = BreakRequest::new(&content, width);
        let (_, broken) = lay_out(&mut self.shaper, &mut self.cache, &content, &request);
        broken
    }
}

#[test]
fn a_cluster_is_eight_wide_and_a_line_sixteen_tall_at_the_initial_size() {
    let mut paragraph = Paragraph::new("abcd", TextStyle::initial());
    let broken = paragraph.run(None);

    assert_eq!(broken.geometry.size.width, CssPx(32.0));
    assert_eq!(broken.geometry.size.height, CssPx(16.0));
    let line = &broken.geometry.lines[0];
    assert_eq!(line.baseline, CssPx(12.8));
    assert_eq!(line.height, CssPx(16.0));
}

#[test]
fn shaping_happens_once_and_breaking_once_per_distinct_width() {
    // The framework's own counters say the same thing and are asserted in `tests/counters.rs`,
    // where every test holds a recording: the counters are one process-wide block, so a measuring
    // test in a binary that also shapes unguarded would read the other test's work.
    let mut paragraph = Paragraph::new("aaa bbb ccc", TextStyle::initial());
    paragraph.run(Some(CssPx(80.0)));
    paragraph.run(Some(CssPx(40.0)));
    paragraph.run(Some(CssPx(40.0))); // the repeat must cost nothing
    paragraph.run(None);

    assert_eq!(paragraph.shaper.shapes(), 1, "one content, one shape");
    assert_eq!(paragraph.shaper.breaks(), 3, "three distinct requests");
}

#[test]
fn a_second_width_never_reshapes_however_many_times_it_is_asked() {
    let mut paragraph = Paragraph::new("aaa bbb ccc ddd", TextStyle::initial());
    for width in [400.0, 200.0, 100.0, 80.0, 60.0, 40.0] {
        for _ in 0..8 {
            paragraph.run(Some(CssPx(width)));
        }
    }
    assert_eq!(paragraph.shaper.shapes(), 1);
    assert_eq!(paragraph.shaper.breaks(), 6);
}

#[test]
fn the_same_paragraph_lays_out_identically_a_hundred_times() {
    let first = Paragraph::new("aaa bbb ccc", TextStyle::initial()).run(Some(CssPx(40.0)));
    for _ in 0..100 {
        let again = Paragraph::new("aaa bbb ccc", TextStyle::initial()).run(Some(CssPx(40.0)));
        assert_eq!(again, first);
    }
    assert_eq!(first.geometry.lines.len(), 3);
}

#[test]
fn breaking_takes_the_last_opportunity_that_fits() {
    let mut paragraph = Paragraph::new("aaa bbb", TextStyle::initial());
    let broken = paragraph.run(Some(CssPx(40.0)));
    assert_eq!(broken.geometry.lines.len(), 2);
    assert_eq!(broken.geometry.lines[0].text, 0..3);
    assert_eq!(broken.geometry.lines[1].text, 3..7);
    assert_eq!(broken.geometry.lines[1].top, CssPx(16.0));
}

#[test]
fn alignment_resolves_start_against_the_base_direction() {
    let mut left = Paragraph::new("ab", TextStyle::initial());
    left.paragraph.align = TextAlign::Start;
    assert_eq!(
        left.run(Some(CssPx(100.0))).geometry.lines[0].offset,
        CssPx(0.0)
    );

    let mut right = Paragraph::new("ab", TextStyle::initial());
    right.paragraph.align = TextAlign::Start;
    right.paragraph.direction = Direction::RightToLeft;
    let broken = right.run(Some(CssPx(100.0)));
    assert_eq!(broken.geometry.lines[0].offset, CssPx(84.0));
    assert!(broken.geometry.is_rtl);

    let mut centred = Paragraph::new("ab", TextStyle::initial());
    centred.paragraph.align = TextAlign::Center;
    assert_eq!(
        centred.run(Some(CssPx(100.0))).geometry.lines[0].offset,
        CssPx(42.0)
    );
}

#[test]
fn an_inline_box_makes_its_line_taller_and_is_placed_on_it() {
    let mut paragraph = Paragraph::new("ab", TextStyle::initial());
    paragraph.boxes = vec![InlineBoxGeometry {
        id: 1,
        offset: 1,
        width: CssPx(20.0),
        height: CssPx(40.0),
        ascent: CssPx(40.0),
        shift: CssPx::ZERO,
    }];

    let broken = paragraph.run(Some(CssPx(200.0)));
    assert_eq!(broken.boxes.len(), 1);
    assert_eq!(broken.geometry.lines[0].height, CssPx(43.2));
    assert_eq!(broken.geometry.lines[0].width, CssPx(36.0));
    assert_eq!(broken.boxes[0].origin.y, CssPx(0.0));
}

#[test]
fn a_vertical_align_shift_moves_the_box_without_reshaping() {
    let mut paragraph = Paragraph::new("ab", TextStyle::initial());
    // The box is shorter than the strut, so the line's height does not follow it and the shift has
    // somewhere to move it to. A box that dominates its line sits at the line's top either way, and
    // a test written over one would be blind to the shift.
    paragraph.boxes = vec![InlineBoxGeometry {
        id: 1,
        offset: 1,
        width: CssPx(20.0),
        height: CssPx(20.0),
        ascent: CssPx(8.0),
        shift: CssPx::ZERO,
    }];
    let flat = paragraph.run(Some(CssPx(200.0)));

    paragraph.boxes[0].shift = CssPx(5.0);
    let raised = paragraph.run(Some(CssPx(200.0)));

    assert_eq!(paragraph.shaper.shapes(), 1, "a shift is not a reshape");
    assert_eq!(paragraph.shaper.breaks(), 2, "but it is a re-break");
    assert_ne!(raised.boxes[0].origin.y, flat.boxes[0].origin.y);
}

#[test]
fn the_strut_is_what_an_empty_paragraph_is_tall() {
    let mut shaper = MonoShaper::new();
    let strut = shaper.strut(&TextStyle::initial());
    assert_eq!(strut.font_ascent, CssPx(12.8));
    assert_eq!(strut.font_descent, CssPx(3.2));
    assert_eq!(strut.line_height, CssPx(16.0));
    assert_eq!(
        strut.ascent(),
        CssPx(12.8),
        "no leading at the normal height"
    );
}

/// Every crate reachable from `root` by following path dependencies, `root` included.
///
/// A one-manifest scan answers "does this crate name a font engine", which is not the question: a
/// dependency two crates away opens font files just as effectively. So the graph is walked.
fn reachable_manifests(root: &Path) -> Vec<(PathBuf, String)> {
    let mut pending = vec![root.to_path_buf()];
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut found = Vec::new();

    while let Some(directory) = pending.pop() {
        let Ok(directory) = directory.canonicalize() else {
            continue;
        };
        if !seen.insert(directory.clone()) {
            continue;
        }
        let manifest = directory.join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest)
            .unwrap_or_else(|error| panic!("{} is unreadable: {error}", manifest.display()));

        for line in text.lines() {
            let Some(rest) = line.split_once("path = \"") else {
                continue;
            };
            let Some((relative, _)) = rest.1.split_once('"') else {
                continue;
            };
            pending.push(directory.join(relative));
        }
        found.push((manifest, text));
    }
    found
}

/// The font engine a manifest names, if it names one.
fn names_a_font_engine(manifest: &str) -> Option<&'static str> {
    [
        "parley",
        "fontique",
        "swash",
        "skrifa",
        "harfrust",
        "freetype",
        "fontconfig",
        "font-kit",
        "fontdb",
        "ab_glyph",
        "rusttype",
    ]
    .into_iter()
    .find(|engine| manifest.contains(engine))
}

#[test]
fn the_font_engine_scan_recognises_one_when_it_sees_it() {
    // The control for the scan below. A predicate that answered "no font engine" for every input
    // would make that test green for ever, and it is exactly the shape a typo in the list produces.
    assert_eq!(
        names_a_font_engine("[dependencies]\nparley.workspace = true\n"),
        Some("parley")
    );
    assert_eq!(
        names_a_font_engine("[dependencies]\nsmallvec = \"1\"\n"),
        None
    );
}

#[test]
fn nothing_reachable_from_this_crate_is_a_font_engine() {
    // The shaper's promise is that the suite runs on a machine with no fonts installed at all. What
    // makes that true is that no font library is *reachable* from here to open one — so the whole
    // path-dependency graph is read rather than the promise being restated in a comment, and rather
    // than one manifest being read and the answer generalised from it.
    let manifests = reachable_manifests(Path::new(env!("CARGO_MANIFEST_DIR")));

    for (path, text) in &manifests {
        assert_eq!(
            names_a_font_engine(text),
            None,
            "a font engine is reachable from the crate whose shaper needs no font files, through {}",
            path.display()
        );
    }

    // The walk itself is checked, because a scan that read one manifest and stopped would report
    // exactly the same clean result. Neither crate below is a direct dependency of this one — they
    // are reachable only through others — so finding both is what says the graph was followed.
    let names: Vec<String> = manifests
        .iter()
        .map(|(path, _)| {
            path.parent()
                .and_then(|directory| directory.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect();
    for expected in ["zgui-interned", "zgui-vocab"] {
        assert!(
            names.iter().any(|name| name == expected),
            "the walk never reached {expected}, so it did not follow the graph; it reached {names:?}"
        );
    }
}
