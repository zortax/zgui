//! The variation instance a shaped run names: the coordinates painting draws its glyphs at.

mod support;

use std::sync::Arc;

use zgui_interned::Ident;
use zgui_text::{BreakRequest, FontSource, ParagraphShaper, ShapedRunOwned};
use zgui_text_parley::{Controls, FontSystem, FontSystemOptions, Shaper};
use zgui_text_style::{Direction, FamilyName, FontFamilyList, TextStyle};

/// The first run of "HOW" shaped in the shipped variable face at `weight`.
fn variable_run(fonts: &Arc<FontSystem>, weight: f32) -> ShapedRunOwned {
    let mut shaper = Shaper::with_controls(fonts.clone(), Controls::Verbatim);
    let mut fixture = support::Fixture::new("HOW", Direction::LeftToRight);
    fixture.runs[0].style = Arc::new(TextStyle {
        family: FontFamilyList::from_iter([FamilyName::Named(Ident::new("Rubik"))]),
        size: zgui_geom::CssPx(48.0),
        weight,
        ..TextStyle::initial()
    });
    let content = fixture.content();
    let mut shaped = shaper.shape(&content);
    shaper.break_lines(&mut shaped, &BreakRequest::new(&content, None));
    let mut runs = Vec::new();
    shaper.visit_line(&shaped, 0, &mut |run| runs.push(ShapedRunOwned::from(run)));
    runs.into_iter().next().expect("one run")
}

/// A font system holding the shipped variable face.
fn variable_fonts() -> Arc<FontSystem> {
    let fonts = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
    fonts
        .register(support::face("RubikSubset-Variable.ttf"), None)
        .expect("the variable face registers");
    fonts
}

/// Two weights of one variable face are one face at two points on its `wght` axis.
#[test]
fn a_variable_face_shapes_each_weight_at_its_own_coordinates() {
    let fonts = variable_fonts();
    let regular = variable_run(&fonts, 400.0);
    let heavy = variable_run(&fonts, 800.0);
    assert_eq!(regular.face, heavy.face, "one file, one face");
    assert!(!heavy.coords.is_empty());
    assert_ne!(
        regular.coords, heavy.coords,
        "two weights of one variable face were shaped at one instance"
    );
    assert_eq!(heavy.synthetic_bold, 0.0, "the weight comes from the axis");
}

/// The default point of a variable face's design space is named by no coordinates at all, as a
/// static face is.
#[test]
fn the_default_instance_carries_no_coordinates() {
    // The face's `wght` axis rests at 300.
    let fonts = variable_fonts();
    assert_eq!(variable_run(&fonts, 300.0).coords, Vec::<i16>::new());

    let (_fonts, mut shaper) = support::shaper(Controls::Verbatim);
    let fixture = support::Fixture::new("H", Direction::LeftToRight);
    let content = fixture.content();
    let mut shaped = shaper.shape(&content);
    shaper.break_lines(&mut shaped, &BreakRequest::new(&content, None));
    let mut coords = Vec::new();
    shaper.visit_line(&shaped, 0, &mut |run| coords.push(run.coords.to_vec()));
    assert_eq!(coords, vec![Vec::<i16>::new()]);
}
