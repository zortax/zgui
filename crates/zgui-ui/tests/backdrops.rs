//! Where a `backdrop-filter` reads and writes, read off the graphics device.
//!
//! A transform moves a box when it is drawn, and its backdrop has to move with it. The fixture puts
//! a blurring box over sharp stripes and moves it sideways with a transform. The stripes stay sharp
//! where layout put the box and turn grey where the transform put it.

mod desktop;
mod device;
mod painted;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::view;
use zgui::view::{AnyView, NodeRef};

use crate::painted::stage::Stage;

/// Stripes four pixels wide, and a blurring box moved 160 pixels to the right.
const SHEET: &str = ":root { background-color: #ffffff }
     .field { position: relative; display: flex; flex-direction: row; width: 400px; height: 120px; margin: 40px }
     .stripe { width: 4px; height: 120px; background-color: #ffffff }
     .stripe.dark { background-color: #000000 }
     .frost {
        position: absolute; left: 20px; top: 20px; width: 80px; height: 80px;
        transform: translateX(160px);
        backdrop-filter: blur(6px);
     }";

/// How many stripes the field holds.
const STRIPES: usize = 100;

/// A pixel that is neither near black nor near white, which only the blur produces.
fn grey((red, green, blue): (u8, u8, u8)) -> bool {
    let level = (u16::from(red) + u16::from(green) + u16::from(blue)) / 3;
    (48..=208).contains(&level)
}

/// The share of the pixels in `rect` that are grey.
fn greys(stage: &Stage, rect: Rect<DevicePx, Device>) -> f32 {
    let colours = stage.colours_in(rect);
    colours.iter().filter(|colour| grey(**colour)).count() as f32 / colours.len().max(1) as f32
}

/// A square of `side` at `x`, `y` from `origin`, shrunk by the blur's reach on every side.
fn inner(origin: Point<DevicePx, Device>, x: f32, y: f32, side: f32) -> Rect<DevicePx, Device> {
    let reach = 20.0;
    Rect::new(
        Point::new(
            DevicePx(origin.x.0 + x + reach),
            DevicePx(origin.y.0 + y + reach),
        ),
        Size::new(DevicePx(side - 2.0 * reach), DevicePx(side - 2.0 * reach)),
    )
}

#[test]
fn a_transformed_backdrop_blurs_where_the_transform_puts_its_box() {
    let field = NodeRef::new();
    let Some(stage) = Stage::open(SHEET, move || {
        let stripes: Vec<AnyView> = (0..STRIPES)
            .map(|index| {
                let class = if index % 2 == 0 {
                    "stripe dark"
                } else {
                    "stripe"
                };
                AnyView::new(view! { box(class = class) })
            })
            .collect();
        AnyView::new(view! {
            box(class = "field", node_ref = field) {
                {stripes}
                box(class = "frost")
            }
        })
    }) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let node = field.get_untracked().expect("the field is built");
    let origin = stage.rect_of(node).origin;

    let moved = greys(&stage, inner(origin, 180.0, 20.0, 80.0));
    let left = greys(&stage, inner(origin, 20.0, 20.0, 80.0));
    assert!(
        moved > 0.9,
        "the box the transform moved is not blurred: {moved} of its pixels are grey"
    );
    assert!(
        left < 0.1,
        "the place layout put the box is blurred: {left} of its pixels are grey"
    );
}

/// The stripes, and a blurring box with round corners where layout put it.
const ROUNDED: &str = ":root { background-color: #ffffff }
     .field { position: relative; display: flex; flex-direction: row; width: 400px; height: 120px; margin: 40px }
     .stripe { width: 4px; height: 120px; background-color: #ffffff }
     .stripe.dark { background-color: #000000 }
     .frost {
        position: absolute; left: 20px; top: 20px; width: 80px; height: 80px;
        border-radius: 30px;
        backdrop-filter: blur(6px);
     }";

#[test]
fn a_rounded_backdrop_leaves_what_is_outside_its_corners_alone() {
    let field = NodeRef::new();
    let Some(stage) = Stage::open(ROUNDED, move || {
        let stripes: Vec<AnyView> = (0..STRIPES)
            .map(|index| {
                let class = if index % 2 == 0 {
                    "stripe dark"
                } else {
                    "stripe"
                };
                AnyView::new(view! { box(class = class) })
            })
            .collect();
        AnyView::new(view! {
            box(class = "field", node_ref = field) {
                {stripes}
                box(class = "frost")
            }
        })
    }) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let node = field.get_untracked().expect("the field is built");
    let origin = stage.rect_of(node).origin;
    let at = |x: f32, y: f32, side: f32| {
        Rect::new(
            Point::new(DevicePx(origin.x.0 + x), DevicePx(origin.y.0 + y)),
            Size::new(DevicePx(side), DevicePx(side)),
        )
    };

    let middle = greys(&stage, inner(origin, 20.0, 20.0, 80.0));
    assert!(
        middle > 0.9,
        "the box is blurred: {middle} of its pixels are grey"
    );
    // Each square is inside the border box and outside the 30-pixel corner curve.
    for (x, y) in [(20.0, 20.0), (94.0, 20.0), (20.0, 94.0), (94.0, 94.0)] {
        let corner = greys(&stage, at(x, y, 6.0));
        assert_eq!(
            corner, 0.0,
            "the corner at {x}, {y} is outside the curve and keeps its stripes"
        );
    }
}
