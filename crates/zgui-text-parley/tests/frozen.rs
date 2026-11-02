//! A frozen paragraph reports what the engine's layout reports, bit for bit.
//!
//! A paragraph whose runs may not wrap, that holds no inline box and whose runs all go left to
//! right is held in a compact frozen form. The proof is here: for random such paragraphs broken
//! at a random sequence of requests, each answer, each glyph and each cluster of the frozen form
//! equals the one of the same paragraph held in the engine's layout. Painting reads a paragraph
//! only through those glyph runs, so equal runs paint equal pixels.

mod support;

use std::sync::{Arc, MutexGuard, OnceLock};

use proptest::prelude::*;
use proptest::strategy::ValueTree;
use zgui_geom::CssPx;
use zgui_profile::{Counter, counter};
use zgui_scene::PaintSlot;
use zgui_text::{
    BreakRequest, BrokenParagraph, InlineBoxGeometry, LineBand, LineBands, ParagraphCache,
    ParagraphContent, ParagraphShaper, ShapedParagraph, StyledRun, TextMap, lay_out,
};
use zgui_text_parley::{Controls, FontSystem, ShapedLayout, Shaper};
use zgui_text_style::{
    Direction, LengthPercent, ParagraphStyle, TextAlign, TextIndent, TextStyle, WrapMode,
};

/// Zeroes the counters and holds them for the duration of one test.
fn counting() -> MutexGuard<'static, ()> {
    let guard = counter::exclusive();
    counter::reset();
    guard
}

/// The shipped faces, loaded once for every case.
fn fonts() -> Arc<FontSystem> {
    static FONTS: OnceLock<Arc<FontSystem>> = OnceLock::new();
    FONTS.get_or_init(support::fonts).clone()
}

/// Words with ligatures, digits, punctuation and Arabic, so that runs split and clusters join.
const WORDS: &[&str] = &[
    "the", "fit", "office", "affluent", "flow", "a", "OK", "1234", "-0.25", "Save…", "ملف",
];

/// What goes between two words. The newline forces a break.
const GAPS: &[&str] = &[" ", " ", "  ", "\u{a0}", "\n", "\t"];

/// One run's style, as drawn.
#[derive(Clone, Debug)]
struct RunSpec {
    size: f32,
    bold: bool,
    letter_spacing: f32,
    brush: u32,
}

/// One request of the sequence.
#[derive(Clone, Debug)]
struct Step {
    width: Option<f32>,
    basis: Option<f32>,
    probe: bool,
    banded: bool,
    realign: Option<TextAlign>,
}

/// One whole case.
#[derive(Clone, Debug)]
struct Case {
    lead: bool,
    words: Vec<(usize, usize)>,
    trail: Option<usize>,
    scale: f32,
    cuts: Vec<f32>,
    runs: Vec<RunSpec>,
    align: TextAlign,
    rtl: bool,
    controls: Controls,
    indent: (f32, f32, bool, bool),
    steps: Vec<Step>,
}

fn alignment() -> impl Strategy<Value = TextAlign> {
    prop::sample::select(vec![
        TextAlign::Start,
        TextAlign::Left,
        TextAlign::End,
        TextAlign::Right,
        TextAlign::Center,
        TextAlign::Center,
        TextAlign::Justify,
    ])
}

/// An index into [`WORDS`], the Arabic word (the last) one time in 30.
fn word() -> impl Strategy<Value = usize> {
    prop_oneof![29 => 0..WORDS.len() - 1, 1 => Just(WORDS.len() - 1)]
}

fn run_spec() -> impl Strategy<Value = RunSpec> {
    (
        prop::sample::select(vec![11.0f32, 13.5, 16.0, 21.0]),
        prop::bool::weighted(0.2),
        prop::sample::select(vec![0.0f32, 0.0, 0.5, -0.4]),
        0u32..3,
    )
        .prop_map(|(size, bold, letter_spacing, brush)| RunSpec {
            size,
            bold,
            letter_spacing,
            brush,
        })
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    prop::collection::vec(
        (
            prop::option::weighted(0.85, 0.0f32..400.0),
            prop::option::weighted(0.3, 0.0f32..400.0),
            prop::bool::weighted(0.3),
            prop::bool::weighted(0.05),
            prop::option::weighted(0.15, alignment()),
        )
            .prop_map(|(width, basis, probe, banded, realign)| Step {
                width,
                basis,
                probe,
                banded,
                realign,
            }),
        1..14,
    )
}

fn case() -> impl Strategy<Value = Case> {
    (
        prop::bool::weighted(0.1),
        prop::collection::vec((word(), 0..GAPS.len()), 0..12),
        prop::option::weighted(0.15, 0..GAPS.len()),
        prop::sample::select(vec![1.0f32, 1.0, 1.25, 2.0]),
        prop::collection::vec(0.0f32..1.0, 0..3),
        prop::collection::vec(run_spec(), 1..4),
        alignment(),
        prop::bool::weighted(0.1),
        prop::sample::select(vec![Controls::Mark, Controls::Mark, Controls::Verbatim]),
        (
            prop::sample::select(vec![0.0f32, 0.0, 12.0, -6.0]),
            prop::sample::select(vec![0.0f32, 0.0, 0.1]),
            prop::bool::ANY,
            prop::bool::ANY,
        ),
        steps(),
    )
        .prop_map(
            |(lead, words, trail, scale, cuts, runs, align, rtl, controls, indent, steps)| Case {
                lead,
                words,
                trail,
                scale,
                cuts,
                runs,
                align,
                rtl,
                controls,
                indent,
                steps,
            },
        )
}

/// A case's content, owned.
struct Built {
    text: String,
    map: TextMap,
    runs: Vec<StyledRun>,
    paragraph: ParagraphStyle,
    scale: f32,
}

impl Built {
    fn new(case: &Case) -> Self {
        let mut text = String::new();
        if case.lead {
            text.push(' ');
        }
        for (index, &(word, gap)) in case.words.iter().enumerate() {
            if index > 0 {
                text.push_str(GAPS[gap]);
            }
            text.push_str(WORDS[word]);
        }
        if let Some(gap) = case.trail {
            text.push_str(GAPS[gap]);
        }
        let starts: Vec<usize> = text
            .char_indices()
            .map(|(at, _)| at)
            .chain([text.len()])
            .collect();
        let mut cuts: Vec<usize> = case
            .cuts
            .iter()
            .map(|&cut| starts[((starts.len() - 1) as f32 * cut).round() as usize])
            .collect();
        cuts.push(0);
        cuts.push(text.len());
        cuts.sort_unstable();
        cuts.dedup();
        // Empty text keeps one empty run, as an empty field has.
        if cuts.len() == 1 {
            cuts.push(0);
        }
        let runs = cuts
            .windows(2)
            .enumerate()
            .map(|(index, range)| {
                let spec = &case.runs[index % case.runs.len()];
                StyledRun {
                    text: range[0]..range[1],
                    style: Arc::new(TextStyle {
                        size: CssPx(spec.size),
                        weight: if spec.bold { 700.0 } else { 400.0 },
                        letter_spacing: LengthPercent::length(CssPx(spec.letter_spacing)),
                        wrap_mode: WrapMode::NoWrap,
                        ..support::style()
                    }),
                    brush: PaintSlot(spec.brush),
                }
            })
            .collect();
        let mut map = TextMap::new();
        map.push(0..text.len(), 0, 0);
        let (length, percent, hanging, each_line) = case.indent;
        Self {
            text,
            map,
            runs,
            paragraph: ParagraphStyle {
                direction: if case.rtl {
                    Direction::RightToLeft
                } else {
                    Direction::LeftToRight
                },
                align: case.align,
                indent: TextIndent {
                    length: LengthPercent {
                        length: CssPx(length),
                        percent,
                    },
                    hanging,
                    each_line,
                },
                ..ParagraphStyle::initial()
            },
            scale: case.scale,
        }
    }

    fn content<'a>(&'a self, paragraph: &'a ParagraphStyle) -> ParagraphContent<'a> {
        ParagraphContent {
            text: &self.text,
            map: &self.map,
            runs: &self.runs,
            boxes: &[],
            paragraph,
            scale: self.scale,
        }
    }
}

/// Every glyph run and cluster run of the form, as text.
///
/// The debug form of a float prints the shortest text that reads back to the same bits, so equal
/// text means equal bits.
fn form(shaper: &Shaper, shaped: &ShapedParagraph<ShapedLayout>, lines: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in 0..=lines {
        let line = line as u16;
        shaper.visit_line(shaped, line, &mut |run| out.push(format!("{run:?}")));
        shaper.visit_clusters(shaped, line, &mut |run| out.push(format!("{run:?}")));
    }
    out
}

/// Bit-for-bit text of a broken result.
fn print(broken: &BrokenParagraph) -> String {
    format!("{broken:?}")
}

/// Breaks `case` at each of its steps in both forms, and compares every answer and every kept form.
///
/// Returns whether the paragraph was frozen when shaped.
fn check(case: &Case) -> Result<bool, TestCaseError> {
    let built = Built::new(case);
    let fonts = fonts();
    let mut frozen = Shaper::with_controls(fonts.clone(), case.controls);
    let mut live = Shaper::with_controls(fonts, case.controls).freezing(false);
    let mut paragraph = built.paragraph;
    let mut cold = frozen.shape(&built.content(&paragraph));
    let mut warm = live.shape(&built.content(&paragraph));
    let was_frozen = cold.engine.is_frozen();
    prop_assert!(!warm.engine.is_frozen());
    prop_assert_eq!(cold.content_widths(), warm.content_widths());

    for step in &case.steps {
        if let Some(align) = step.realign {
            paragraph.align = align;
        }
        let content = built.content(&paragraph);
        let bands = step.width.map(|width| {
            [
                LineBand {
                    offset: CssPx(width * 0.3),
                    max_advance: CssPx(width * 0.7),
                },
                LineBand::full(CssPx(width)),
            ]
        });
        let mut request = BreakRequest::new(&content, step.width.map(CssPx));
        request.indent_basis = step.basis.map(CssPx).or(request.indent_basis);
        if step.banded
            && let Some(bands) = &bands
        {
            request = request.banded(LineBands::new(bands));
        }
        if step.probe {
            request = request.probing();
        }

        let answer = frozen.break_lines(&mut cold, &request);
        let reference = live.break_lines(&mut warm, &request);
        prop_assert_eq!(print(&answer), print(&reference), "the answers differ");
        if !step.probe {
            let lines = reference.geometry.lines.len();
            prop_assert_eq!(
                form(&frozen, &cold, lines),
                form(&live, &warm, lines),
                "the glyphs or clusters differ"
            );
        }
    }
    Ok(was_frozen)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 512,
        ..ProptestConfig::default()
    })]

    /// Every answer, glyph and cluster of a frozen paragraph equals the live one, bit for bit.
    #[test]
    fn a_frozen_paragraph_reads_as_the_live_one(case in case()) {
        let _guard = counting();
        check(&case)?;
    }
}

/// One single-run label in the initial alignment, shaped by `shaper`.
fn label(
    shaper: &mut Shaper,
    text: &str,
    wrap_mode: WrapMode,
    direction: Direction,
) -> (ShapedParagraph<ShapedLayout>, BrokenParagraph) {
    let mut map = TextMap::new();
    map.push(0..text.len(), 0, 0);
    let runs = [StyledRun {
        text: 0..text.len(),
        style: Arc::new(TextStyle {
            wrap_mode,
            ..support::style()
        }),
        brush: PaintSlot(0),
    }];
    let paragraph = ParagraphStyle {
        direction,
        ..ParagraphStyle::initial()
    };
    let content = ParagraphContent {
        text,
        map: &map,
        runs: &runs,
        boxes: &[],
        paragraph: &paragraph,
        scale: 1.0,
    };
    let mut shaped = shaper.shape(&content);
    let broken = shaper.break_lines(&mut shaped, &BreakRequest::new(&content, Some(CssPx(40.0))));
    (shaped, broken)
}

/// Labels that cannot wrap are frozen; the others keep the engine's layout.
#[test]
fn only_a_paragraph_that_breaks_one_way_is_frozen() {
    let _guard = counting();
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let frozen = |shaper: &mut Shaper, text, wrap, direction| {
        label(shaper, text, wrap, direction).0.engine.is_frozen()
    };
    assert!(frozen(
        &mut shaper,
        "Save changes",
        WrapMode::NoWrap,
        Direction::LeftToRight
    ));
    assert!(frozen(
        &mut shaper,
        "one\ntwo",
        WrapMode::NoWrap,
        Direction::LeftToRight
    ));
    assert!(!frozen(
        &mut shaper,
        "Save changes",
        WrapMode::Wrap,
        Direction::LeftToRight
    ));
    assert!(!frozen(
        &mut shaper,
        "ملف تقرير",
        WrapMode::NoWrap,
        Direction::LeftToRight
    ));
    assert!(!frozen(
        &mut shaper,
        "Save",
        WrapMode::NoWrap,
        Direction::RightToLeft
    ));
    assert_eq!(counter::get(Counter::TextFrozen), 2);
    // An empty field, and a last line with no clusters.
    assert!(frozen(
        &mut shaper,
        "",
        WrapMode::NoWrap,
        Direction::LeftToRight
    ));
    assert!(frozen(
        &mut shaper,
        "one\n",
        WrapMode::NoWrap,
        Direction::LeftToRight
    ));

    // Without a directional mark, a leading space is the first cluster the breaker reads.
    let (_fonts, mut verbatim) = support::shaper(Controls::Verbatim);
    assert!(!frozen(
        &mut verbatim,
        " Save",
        WrapMode::NoWrap,
        Direction::LeftToRight
    ));
    assert!(frozen(
        &mut verbatim,
        "Save",
        WrapMode::NoWrap,
        Direction::LeftToRight
    ));
}

/// A frozen label holds fewer heap bytes than the engine's layout of it, and remembers no probe.
#[test]
fn a_frozen_label_is_smaller_and_remembers_nothing() {
    let _guard = counting();
    let (_fonts, shaper) = support::shaper(Controls::Mark);
    let mut frozen = shaper;
    let mut live = Shaper::with_controls(fonts(), Controls::Mark).freezing(false);
    let text = "Open recent file";
    let (mut cold, _) = label(&mut frozen, text, WrapMode::NoWrap, Direction::LeftToRight);
    let (warm, _) = label(&mut live, text, WrapMode::NoWrap, Direction::LeftToRight);
    assert!(
        frozen.engine_bytes(&cold) * 2 < live.engine_bytes(&warm),
        "frozen {} bytes against live {}",
        frozen.engine_bytes(&cold),
        live.engine_bytes(&warm)
    );

    let style = Arc::new(TextStyle {
        wrap_mode: WrapMode::NoWrap,
        ..support::style()
    });
    let mut map = TextMap::new();
    map.push(0..text.len(), 0, 0);
    let runs = [StyledRun {
        text: 0..text.len(),
        style,
        brush: PaintSlot(0),
    }];
    let paragraph = ParagraphStyle::initial();
    let content = ParagraphContent {
        text,
        map: &map,
        runs: &runs,
        boxes: &[],
        paragraph: &paragraph,
        scale: 1.0,
    };
    let rebroken = counter::get(Counter::TextRebroken);
    for width in [None, Some(CssPx(0.0)), Some(CssPx(300.0))] {
        frozen.break_lines(&mut cold, &BreakRequest::new(&content, width).probing());
    }
    assert_eq!(cold.remembered(), 0);
    assert_eq!(
        cold.broken_as(),
        Some(BreakRequest::new(&content, Some(CssPx(40.0))).key())
    );
    assert_eq!(
        counter::get(Counter::TextRebroken),
        rebroken,
        "a probe of a frozen paragraph takes no pass"
    );
}

/// A banded request shapes a frozen paragraph again into the engine's layout, and is answered as
/// the live paragraph answers it.
#[test]
fn a_banded_request_thaws_the_paragraph() {
    let _guard = counting();
    let (_fonts, mut frozen) = support::shaper(Controls::Mark);
    let mut live = Shaper::with_controls(fonts(), Controls::Mark).freezing(false);
    let text = "Recent\nfiles";
    let (mut cold, _) = label(&mut frozen, text, WrapMode::NoWrap, Direction::LeftToRight);
    let (mut warm, _) = label(&mut live, text, WrapMode::NoWrap, Direction::LeftToRight);
    assert!(cold.engine.is_frozen());

    let style = Arc::new(TextStyle {
        wrap_mode: WrapMode::NoWrap,
        ..support::style()
    });
    let mut map = TextMap::new();
    map.push(0..text.len(), 0, 0);
    let runs = [StyledRun {
        text: 0..text.len(),
        style,
        brush: PaintSlot(0),
    }];
    let paragraph = ParagraphStyle::initial();
    let content = ParagraphContent {
        text,
        map: &map,
        runs: &runs,
        boxes: &[] as &[InlineBoxGeometry],
        paragraph: &paragraph,
        scale: 1.0,
    };
    let bands = [
        LineBand {
            offset: CssPx(30.0),
            max_advance: CssPx(70.0),
        },
        LineBand::full(CssPx(100.0)),
    ];
    let request = BreakRequest::new(&content, Some(CssPx(100.0))).banded(LineBands::new(&bands));
    let answer = frozen.break_lines(&mut cold, &request);
    assert!(!cold.engine.is_frozen());
    assert_eq!(counter::get(Counter::TextThawed), 1);
    assert_eq!(
        print(&answer),
        print(&live.break_lines(&mut warm, &request))
    );
    assert_eq!(form(&frozen, &cold, 2), form(&live, &warm, 2));
}

/// A paragraph restyled from `nowrap` to `wrap` breaks as a paragraph shaped cold under `wrap`,
/// whether the cache held it frozen or live.
#[test]
fn a_wrap_restyle_breaks_as_a_cold_shape() {
    let _guard = counting();
    let text = "Open recent file";
    let mut map = TextMap::new();
    map.push(0..text.len(), 0, 0);
    let runs_in = |wrap_mode| {
        [StyledRun {
            text: 0..text.len(),
            style: Arc::new(TextStyle {
                wrap_mode,
                ..support::style()
            }),
            brush: PaintSlot(0),
        }]
    };
    let (nowrap, wrap) = (runs_in(WrapMode::NoWrap), runs_in(WrapMode::Wrap));
    let paragraph = ParagraphStyle::initial();
    let content = |runs| ParagraphContent {
        text,
        map: &map,
        runs,
        boxes: &[],
        paragraph: &paragraph,
        scale: 1.0,
    };
    let width = Some(CssPx(40.0));
    let mut cold = Shaper::with_controls(fonts(), Controls::Mark);
    let mut fresh = ParagraphCache::new();
    let wrapped = content(&wrap);
    let (_, expected) = lay_out(
        &mut cold,
        &mut fresh,
        &wrapped,
        &BreakRequest::new(&wrapped, width),
    );
    assert!(expected.geometry.lines.len() > 1, "the label wraps at 40");

    for freezing in [true, false] {
        let mut shaper = Shaper::with_controls(fonts(), Controls::Mark).freezing(freezing);
        let mut cache = ParagraphCache::new();
        let unwrapped = content(&nowrap);
        let (shaped, before) = lay_out(
            &mut shaper,
            &mut cache,
            &unwrapped,
            &BreakRequest::new(&unwrapped, width),
        );
        assert_eq!(shaped.engine.is_frozen(), freezing);
        assert_eq!(before.geometry.lines.len(), 1);
        let (_, after) = lay_out(
            &mut shaper,
            &mut cache,
            &wrapped,
            &BreakRequest::new(&wrapped, width),
        );
        assert_eq!(print(&after), print(&expected), "freezing {freezing}");
    }
}

/// Fixed seeds of the property, so that each run checks frozen paragraphs at all.
#[test]
fn the_property_sees_frozen_paragraphs() {
    let _guard = counting();
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let mut frozen = 0;
    for _ in 0..64 {
        let case = case().new_tree(&mut runner).expect("a case").current();
        if check(&case).expect("frozen and live agree") {
            frozen += 1;
        }
    }
    assert!(frozen > 32, "only {frozen} of 64 cases were frozen");
}
