//! What colour an inline text is drawn in after its own colour moves.
//!
//! An inline text inside a block has no rectangle of its own size. The lines of the block draw its
//! glyphs, and their paint record holds the colour of the slot as it was when the record was
//! encoded. A colour that its slot takes in place must therefore reach those lines in the same
//! frame, with no other change to draw them again.

mod support;

use zgui_interned::{ClassName, CustomPropertyName};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::*;
use zgui_view::{BuildCx, IntoView, View};

/// A block that holds one inline text, coloured through a property the text declares itself.
const CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   .holder { display: block }
                   text { display: inline }
                   .c { color: var(--grey) }";

/// The grey of every glyph the window draws.
fn drawn_greys(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Vec<u8> {
    harness.app().windows()[0]
        .scene()
        .primitives
        .mono_sprites
        .iter()
        .map(|sprite| (sprite.color[0] * 255.0).round() as u8)
        .collect()
}

fn inline_text(cx: &mut BuildCx<'_>, grey: RwSignal<u8>) -> Box<dyn zgui_view::Anchor> {
    let view = zgui_elements::column()
        .child(
            zgui_elements::r#box()
                .class_toggle(ClassName::new("holder"), true)
                .child(
                    zgui_elements::text()
                        .class_toggle(ClassName::new("c"), true)
                        .custom_property(CustomPropertyName::new("grey"), move || {
                            let level = grey.get();
                            Some(format!("rgb({level}, {level}, {level})"))
                        })
                        .child("aa"),
                ),
        )
        .into_view();
    Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
}

#[test]
fn an_inline_text_is_drawn_in_its_new_colour_in_the_same_frame() {
    let grey = RwSignal::new(30_u8);
    let mut harness =
        support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| inline_text(cx, grey));
    harness.settle(8);
    assert_eq!(drawn_greys(&harness), [30; 2]);

    grey.set(210);
    harness.settle(8);
    assert_eq!(
        drawn_greys(&harness),
        [210; 2],
        "the lines replayed the colour they were encoded with"
    );

    grey.set(60);
    harness.settle(8);
    assert_eq!(drawn_greys(&harness), [60; 2]);
    harness.shut_down();
}
