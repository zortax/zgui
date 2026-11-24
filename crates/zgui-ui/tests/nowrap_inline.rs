//! A line that may not wrap stays one line when the text inside it sits in nested inline elements.
//!
//! The shaper marks a break after every inline box, and the edges of a nested inline element reach
//! it as inline boxes. A line that overflows its box must still stay whole, so that
//! `text-overflow` cuts it off at the box edge.

mod desktop;

use zgui::geom::{Device, DevicePx, Rect};
use zgui::view;
use zgui::view::NodeRef;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::desktop::renderer;
use crate::desktop::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff; font-size: 12px }
    .row { display: flex; flex-direction: row; align-items: center; gap: 7px; width: 360px; height: 22px; overflow: hidden }
    .mark { flex: 0 0 auto; width: 12px; height: 12px }
    .label { flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap }
    .lit { font-weight: 600 }
    .note { flex: 1 1 0; min-width: 0; white-space: nowrap; overflow: hidden; text-align: right }
    .field-row { width: 160px }";

/// Text far wider than the row, with a break opportunity after every slash.
const LONG: &str = "try.k8s.io/ingress-nginx/controller:v1.9.4@sha256:abcdef0123456789abcdef";

fn glyphs_in_band(
    sprites: &[Rect<DevicePx, Device>],
    box_rect: Rect<DevicePx, Device>,
) -> Vec<Rect<DevicePx, Device>> {
    sprites
        .iter()
        .copied()
        .filter(|r| r.origin.y.0 >= box_rect.origin.y.0 && r.bottom().0 <= box_rect.bottom().0)
        .collect()
}

#[test]
fn an_overflowing_label_of_nested_runs_stays_one_line() {
    let nested = NodeRef::new();
    let single = NodeRef::new();
    let bare = NodeRef::new();
    let placeholder_field = NodeRef::new();
    let value_field = NodeRef::new();
    let stage = Stage::open(SHEET, move || {
        view! {
            ThemeProvider(scheme = ColorScheme::Light) {
                column {
                    row(class = "row") {
                        box(class = "mark") {}
                        box(class = "label", node_ref = nested) {
                            text(class = "lit") {"regis"}
                            text {{LONG}}
                        }
                        text(class = "note") {"3 pods"}
                    }
                    row(class = "row") {
                        box(class = "mark") {}
                        box(class = "label", node_ref = single) {
                            text {{LONG}}
                        }
                    }
                    row(class = "row") {
                        box(class = "mark") {}
                        text(class = "label", node_ref = bare) {{LONG}}
                    }
                    box(class = "field-row") {
                        Input(placeholder = LONG, node_ref = placeholder_field)
                    }
                    box(class = "field-row") {
                        Input(
                            value = zgui::prelude::RwSignal::new_local("try".to_owned()),
                            node_ref = value_field,
                        )
                    }
                }
            }
        }
    });
    let host = &stage.handles().host;
    let height = |node: NodeRef| {
        let node = node.get_untracked().expect("the label is in the tree");
        host.window_box(node)
            .expect("the label has a box")
            .size
            .height
            .0
    };
    let line = height(bare);
    assert!(line > 0.0);
    assert_eq!(height(single), line, "one nested run stays on one line");
    assert_eq!(height(nested), line, "several nested runs stay on one line");

    let sprites = renderer::last_sprite_bounds();
    let ph_box = host
        .window_box(
            placeholder_field
                .get_untracked()
                .expect("the placeholder input is in the tree"),
        )
        .expect("the placeholder input has a box");
    let val_box = host
        .window_box(
            value_field
                .get_untracked()
                .expect("the value input is in the tree"),
        )
        .expect("the value input has a box");
    assert_eq!(ph_box.size.width.0, 160.0);
    assert_eq!(ph_box.size.height.0, 34.0);

    let ph_glyphs = glyphs_in_band(&sprites, ph_box);
    let val_glyphs = glyphs_in_band(&sprites, val_box);
    assert!(!ph_glyphs.is_empty(), "placeholder renders glyphs");
    assert!(val_glyphs.len() >= 3, "value renders its glyphs");

    // InputStyle has 1px border + 12px horizontal padding on each side (13px inset).
    let content_left = ph_box.origin.x.0 + 13.0;
    let content_right = ph_box.right().0 - 13.0;
    let min_x = ph_glyphs
        .iter()
        .map(|r| r.origin.x.0)
        .fold(f32::INFINITY, f32::min);
    let max_x = ph_glyphs
        .iter()
        .map(|r| r.right().0)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        min_x >= content_left - 1.0,
        "placeholder starts inside left padding ({min_x} < {content_left})"
    );
    assert!(
        max_x <= content_right + 0.5,
        "overflowing placeholder ellipsizes inside right content edge ({max_x} > {content_right})"
    );

    // Placeholder glyphs stay vertically aligned with typed text inside the field.
    let ph_rel_ys: Vec<f32> = ph_glyphs
        .iter()
        .take(3)
        .map(|r| r.origin.y.0 - ph_box.origin.y.0)
        .collect();
    let val_rel_ys: Vec<f32> = val_glyphs
        .iter()
        .take(3)
        .map(|r| r.origin.y.0 - val_box.origin.y.0)
        .collect();
    assert_eq!(
        ph_rel_ys, val_rel_ys,
        "placeholder glyphs stay vertically aligned with typed input text"
    );
}
