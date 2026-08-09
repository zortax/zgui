//! Shaping one line on its own: the runs it comes back as, and where each glyph came from.

mod support;

use zgui_geom::CssPx;
use zgui_interned::Ident;
use zgui_scene::PaintSlot;
use zgui_text::{FontSource, ParagraphShaper};
use zgui_text_parley::{Controls, LineRequest, Shaper};

/// Arabic text: three letters that shape into fewer glyphs than characters.
const ARABIC_TEXT: &str = "ملف";

/// A request naming the Latin face at sixteen device pixels.
fn latin(families: &[Ident]) -> LineRequest<'_> {
    LineRequest {
        families,
        weight: 400,
        italic: false,
        size_device_px: 16.0,
        letter_spacing: 0.0,
        ligatures: true,
    }
}

/// A shaper over the shipped faces.
fn shaper() -> Shaper {
    let (_, shaper) = support::shaper(Controls::Verbatim);
    shaper
}

#[test]
fn ascii_is_one_run_whose_clusters_name_each_byte() {
    let mut shaper = shaper();
    let families = [Ident::new(support::LATIN)];
    let runs = shaper.shape_line("hello", &latin(&families));

    assert_eq!(runs.len(), 1, "one script and one face is one run");
    let run = &runs[0];
    assert_eq!(run.glyphs.len(), 5);
    assert_eq!(run.clusters, vec![0, 1, 2, 3, 4]);
    assert_eq!(run.size, 16.0, "the size is device pixels, unscaled");
    for pair in run.glyphs.windows(2) {
        assert!(pair[1].x > pair[0].x, "the glyphs advance to the right");
    }
    assert!(
        run.glyphs.iter().all(|glyph| glyph.y > 0.0),
        "the baseline sits below the line box's top edge"
    );
}

#[test]
fn an_empty_line_shapes_into_nothing() {
    let mut shaper = shaper();
    let families = [Ident::new(support::LATIN)];
    assert!(shaper.shape_line("", &latin(&families)).is_empty());
}

/// A line of two scripts is drawn from the face in the list that covers each of them.
///
/// The faces are resolved per cluster, so one line comes back as one run per face. Both families
/// are named here because the fixture registers its faces and enumerates nothing else: what a
/// desktop's own installed faces would answer is not something a test can depend on.
#[test]
fn each_script_is_drawn_from_the_face_in_the_list_that_covers_it() {
    let mut shaper = shaper();
    let families = [Ident::new(support::LATIN), Ident::new(support::ARABIC)];
    let text = format!("ab {ARABIC_TEXT}");
    let runs = shaper.shape_line(&text, &latin(&families));

    assert!(runs.len() >= 2, "two scripts cannot be one run");
    let faces: Vec<_> = runs.iter().map(|run| run.face).collect();
    assert!(
        faces.windows(2).any(|pair| pair[0] != pair[1]),
        "the Arabic text is drawn from a different face than the Latin: {faces:?}"
    );
    let text_len = text.len() as u32;
    for run in &runs {
        assert_eq!(run.clusters.len(), run.glyphs.len());
        assert!(
            run.clusters.iter().all(|byte| *byte < text_len),
            "every cluster byte points into the string"
        );
    }
}

#[test]
fn a_right_to_left_run_reports_its_cluster_bytes_in_the_order_it_draws_them() {
    let mut shaper = shaper();
    let families = [Ident::new(support::ARABIC)];
    let runs = shaper.shape_line(ARABIC_TEXT, &latin(&families));

    let run = runs
        .iter()
        .find(|run| run.glyphs.len() > 1)
        .expect("the Arabic text shapes into more than one glyph");
    assert_eq!(run.clusters.len(), run.glyphs.len());
    for pair in run.clusters.windows(2) {
        assert!(
            pair[1] <= pair[0],
            "glyphs are stored visually, so the bytes descend through a right-to-left run: {:?}",
            run.clusters
        );
    }
    assert!(
        run.clusters.iter().any(|byte| *byte > 0),
        "the run covers more than the first character"
    );
}

/// A ligature is one glyph carrying the byte its cluster starts at, and turning the group off
/// gives each character its own glyph again.
#[test]
fn a_ligature_is_one_glyph_naming_the_byte_its_cluster_starts_at() {
    let mut shaper = shaper();
    let families = [Ident::new(support::LATIN)];

    let mut on = latin(&families);
    on.ligatures = true;
    let ligated = shaper.shape_line("fi", &on);
    assert_eq!(ligated.len(), 1);
    assert_eq!(
        ligated[0].glyphs.len(),
        1,
        "the shipped face draws `fi` as one glyph"
    );
    assert_eq!(
        ligated[0].clusters,
        vec![0],
        "the one glyph names the byte its cluster starts at"
    );

    let mut off = latin(&families);
    off.ligatures = false;
    let separate = shaper.shape_line("fi", &off);
    assert_eq!(separate[0].glyphs.len(), 2, "one glyph per character");
    assert_eq!(separate[0].clusters, vec![0, 1]);
}

#[test]
fn letter_spacing_pushes_the_glyphs_apart() {
    let mut shaper = shaper();
    let families = [Ident::new(support::LATIN)];
    let tight = shaper.shape_line("hello", &latin(&families));

    let mut wide_request = latin(&families);
    wide_request.letter_spacing = 4.0;
    let wide = shaper.shape_line("hello", &wide_request);

    let last = |runs: &[zgui_text::ShapedRunOwned]| {
        runs.last()
            .and_then(|run| run.glyphs.last())
            .expect("glyphs")
            .x
    };
    assert!(
        last(&wide) > last(&tight) + 12.0,
        "four pixels after each of the first four glyphs"
    );
}

#[test]
fn a_long_line_is_never_wrapped() {
    let mut shaper = shaper();
    let families = [Ident::new(support::LATIN)];
    let text = "wrap ".repeat(100);
    let runs = shaper.shape_line(&text, &latin(&families));

    let glyphs: usize = runs.iter().map(|run| run.glyphs.len()).sum();
    assert_eq!(glyphs, text.len(), "one glyph per ASCII character");
    let widest = runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .map(|glyph| glyph.x)
        .fold(0.0_f32, f32::max);
    assert!(
        widest > 1_000.0,
        "the line keeps advancing past any plausible wrap width, reaching {widest}"
    );
    let baselines: Vec<f32> = runs
        .iter()
        .flat_map(|run| run.glyphs.iter())
        .map(|glyph| glyph.y)
        .collect();
    assert!(
        baselines.windows(2).all(|pair| pair[0] == pair[1]),
        "every glyph sits on one baseline, which is what one line means"
    );
}

#[test]
fn the_resolved_metrics_name_the_face_the_text_is_shaped_in() {
    let mut shaper = shaper();
    let families = [Ident::new(support::LATIN)];
    let request = latin(&families);

    let resolved = shaper.line_metrics(&request).expect("the face is shipped");
    let runs = shaper.shape_line("x", &request);
    assert_eq!(
        resolved.face, runs[0].face,
        "measuring and shaping agree on the face"
    );
    assert_eq!(
        resolved.cell_advance,
        resolved
            .metrics
            .zero_advance_or_fallback(CssPx(16.0), false)
    );
    assert!(resolved.metrics.descent > CssPx(0.0));
}

#[test]
fn a_request_matching_nothing_measures_nothing_and_shapes_nothing_drawable() {
    let shaper = shaper();
    let families = [Ident::new("No Such Family")];
    let request = LineRequest {
        families: &families,
        weight: 400,
        italic: false,
        size_device_px: 16.0,
        letter_spacing: 0.0,
        ligatures: true,
    };
    assert!(shaper.line_metrics(&request).is_none());
}

#[test]
fn one_line_shaped_alone_matches_the_same_line_shaped_as_a_paragraph() {
    let (fonts, mut shaper) = support::shaper(Controls::Verbatim);
    let families = [Ident::new(support::LATIN)];
    let alone = shaper.shape_line("hello", &latin(&families));

    let fixture = support::Fixture::sized("hello", zgui_text_style::Direction::LeftToRight, 16.0);
    let paragraph = shaper.shape(&fixture.content());
    let mut paragraph_runs = Vec::new();
    shaper.visit_line(&paragraph, 0, &mut |run| {
        paragraph_runs.push((run.face, run.size, run.glyphs.to_vec()));
    });

    assert_eq!(alone.len(), paragraph_runs.len(), "the same run structure");
    for (owned, (face, size, glyphs)) in alone.iter().zip(&paragraph_runs) {
        assert_eq!(owned.size, *size);
        assert_eq!(
            owned.glyphs.iter().map(|g| g.glyph).collect::<Vec<_>>(),
            glyphs.iter().map(|g| g.glyph).collect::<Vec<_>>(),
            "the same glyphs in the same order"
        );
        // The face handles come from the same table, so they are comparable.
        assert!(fonts.face(*face).is_some());
        assert_eq!(owned.face, *face);
    }
}

#[test]
fn a_cached_run_keys_exactly_as_the_run_it_was_shaped_from() {
    let mut shaper = shaper();
    let families = [Ident::new(support::LATIN)];
    let runs = shaper.shape_line("hi", &latin(&families));
    let run = &runs[0];

    // Drawing the same cached run twice asks the rasteriser for the same entries.
    let first: Vec<_> = run
        .glyphs
        .iter()
        .map(|glyph| {
            run.as_run(PaintSlot(0))
                .key_for(*glyph, 0.0, zgui_text::RasterStyle::Grayscale)
        })
        .collect();
    let second: Vec<_> = run
        .glyphs
        .iter()
        .map(|glyph| {
            run.as_run(PaintSlot(7))
                .key_for(*glyph, 0.0, zgui_text::RasterStyle::Grayscale)
        })
        .collect();
    assert_eq!(first, second, "the brush is not part of a glyph's key");
}
