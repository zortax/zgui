//! Registration, resolution, and the mode that makes a rendering reproducible.

mod support;

use std::sync::Arc;

use zgui_geom::CssPx;
use zgui_interned::Ident;
use zgui_text::{FaceQuery, FontSource};
use zgui_text_parley::{ColorSupport, Enumeration, FontSystem, FontSystemOptions};
use zgui_text_style::{
    FamilyName, FontFamilyList, FontSlant, GenericFamily, LineHeight, TextStyle,
};

/// A style asking for one named family and nothing else.
fn asking_for(family: &str) -> TextStyle {
    TextStyle {
        family: FontFamilyList::from_iter([FamilyName::Named(Ident::new(family))]),
        line_height: LineHeight::Normal,
        ..TextStyle::initial()
    }
}

/// With enumeration off, nothing resolves until something is registered.
///
/// This is the property every reference image and every measured advance rests on: what the
/// machine happens to have installed cannot reach the collection at all, so the same registrations
/// produce the same answers anywhere.
#[test]
fn nothing_resolves_before_registration() {
    let fonts = FontSystem::new(FontSystemOptions::registered_only());
    let style = TextStyle::initial();
    assert_eq!(fonts.resolve(&FaceQuery::of(&style)), None);

    // A family the running machine almost certainly has installed is equally invisible.
    let installed = asking_for("DejaVu Sans");
    assert_eq!(fonts.resolve(&FaceQuery::of(&installed)), None);

    fonts
        .register(support::face("NotoSans-Regular.ttf"), None)
        .expect("registers");
    assert!(fonts.resolve(&FaceQuery::of(&style)).is_some());
    assert_eq!(
        fonts.resolve(&FaceQuery::of(&installed)),
        None,
        "and registering one family does not conjure another"
    );
}

/// A registered family fills every generic role nothing is bound to.
///
/// A collection with no system behind it starts with no generic bindings, so `font-family: serif`
/// — which is what an unstyled document resolves to — would find nothing and every font-relative
/// unit in the document would take its fallback branch.
#[test]
fn registration_binds_the_empty_generic_roles() {
    let fonts = FontSystem::new(FontSystemOptions::registered_only());
    assert_eq!(fonts.generic_family(GenericFamily::SansSerif), None);

    fonts
        .register(support::face("NotoSans-Regular.ttf"), None)
        .expect("registers");
    assert_eq!(
        fonts.generic_family(GenericFamily::SansSerif),
        Some(Ident::new(support::LATIN))
    );
    assert_eq!(
        fonts.generic_family(GenericFamily::Serif),
        Some(Ident::new(support::LATIN)),
        "every empty role, not only the obvious one"
    );

    // A second family does not displace the first.
    fonts
        .register(support::face("NotoSansArabic-Regular.ttf"), None)
        .expect("registers");
    assert_eq!(
        fonts.generic_family(GenericFamily::SansSerif),
        Some(Ident::new(support::LATIN))
    );
}

/// A face can be registered under a name of the caller's choosing, which `@font-face` needs.
#[test]
fn a_face_can_be_registered_under_another_name() {
    let fonts = FontSystem::new(FontSystemOptions::registered_only());
    fonts
        .register(
            support::face("NotoSans-Regular.ttf"),
            Some(Ident::new("Brand")),
        )
        .expect("registers");

    let branded = asking_for("Brand");
    let face = fonts
        .resolve(&FaceQuery::of(&branded))
        .expect("the family answers to the name it was given");
    let record = fonts.face(face).expect("the handle describes a face");
    assert_eq!(record.family, Ident::new("Brand"));
    assert_eq!(record.weight, 400.0);
    assert_eq!(record.slant, FontSlant::Upright);
    assert!(!record.has_color);
}

/// Bytes that are not a font are refused rather than registered as an empty family.
#[test]
fn unreadable_bytes_are_refused() {
    let fonts = FontSystem::new(FontSystemOptions::registered_only());
    let rubbish: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(vec![0u8; 64]);
    assert!(fonts.register(rubbish, None).is_err());
    let empty: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(Vec::new());
    assert!(fonts.register(empty, None).is_err());
}

/// Fallback is per character: an Arabic word finds the Arabic face even when the run asks for the
/// Latin one.
#[test]
fn fallback_is_per_character() {
    let fonts = support::fonts();
    let latin_only = asking_for(support::LATIN);
    let query = FaceQuery::of(&latin_only);

    let for_latin = fonts
        .resolve_for(&query, 'a')
        .expect("the Latin face draws Latin");
    let for_arabic = fonts
        .resolve_for(&query, 'م')
        .expect("something in the collection draws Arabic");
    assert_ne!(
        for_latin, for_arabic,
        "a run of Latin with one Arabic word in it resolves to two faces"
    );
    assert_eq!(
        fonts.face(for_arabic).expect("described").family,
        Ident::new(support::ARABIC)
    );
}

/// The colour probe reads the face's tables rather than its name.
#[test]
fn colour_support_is_probed_from_the_tables() {
    let text = support::face("NotoSans-Regular.ttf");
    let colour = support::face("NotoZnamennyMusicalNotation-Regular.ttf");
    assert_eq!(
        ColorSupport::probe((*text).as_ref(), 0),
        ColorSupport::NONE,
        "an ordinary text face carries no colour glyphs"
    );
    let support = ColorSupport::probe((*colour).as_ref(), 0);
    assert!(
        support.outlines,
        "the shipped colour face carries layered colour outlines"
    );
    assert!(support.any());

    let fonts = FontSystem::new(FontSystemOptions::registered_only());
    fonts.register(colour, None).expect("registers");
    let style = asking_for(support::COLOR);
    let face = fonts.resolve(&FaceQuery::of(&style)).expect("resolves");
    assert!(
        fonts.face(face).expect("described").has_color,
        "and the record says so, which is what decides the rasterisation path"
    );
}

/// A face registered after a shaper was built is visible to it, because the collection is shared.
#[test]
fn a_face_registered_later_reaches_the_shaper() {
    use zgui_text::ParagraphShaper;
    use zgui_text_parley::Shaper;

    let fonts = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
    let mut shaper = Shaper::new(fonts.clone());
    let style = asking_for(support::LATIN);
    let before = shaper.strut(&style);
    assert_eq!(before.font_ascent, CssPx(0.0), "no face yet");

    fonts
        .register(support::face("NotoSans-Regular.ttf"), None)
        .expect("registers");
    shaper.forget_measurements();
    let after = shaper.strut(&style);
    assert!(
        after.font_ascent.0 > 0.0,
        "the shaper's view of the collection must include what was registered after it was built"
    );
}

/// The two constructors select the two modes, and they disagree about reading the machine.
///
/// This is the plumbing half of the choice and it is the only half that can be asserted from a
/// test at all: what [`Enumeration::System`] finds is whatever the running machine has installed,
/// so a test that asserted it found anything would be asserting a property of the machine. What is
/// ours, and what this pins, is that the constructor a caller names decides the flag the
/// collection is built with — a `with_system_fonts` that silently produced a registered-only
/// collection would leave every application with no fonts and no test anywhere would notice.
#[test]
fn the_constructors_select_the_two_enumeration_modes() {
    assert_eq!(
        FontSystemOptions::registered_only().enumeration,
        Enumeration::Registered
    );
    assert_eq!(
        FontSystemOptions::with_system_fonts().enumeration,
        Enumeration::System
    );

    assert!(
        !FontSystemOptions::registered_only()
            .enumeration
            .reads_the_system()
    );
    assert!(
        FontSystemOptions::with_system_fonts()
            .enumeration
            .reads_the_system()
    );

    // The default is the reproducible mode, so a caller who names nothing gets no machine in the
    // answer. Everything else in this file depends on that and would otherwise pass by luck.
    assert_eq!(
        FontSystemOptions::default(),
        FontSystemOptions::registered_only()
    );
}
