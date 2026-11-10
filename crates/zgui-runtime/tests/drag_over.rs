//! Content dragged from outside the window tells the elements it is over, and they can show it.
//!
//! A drag entering a zone fires `drag_enter` on it, every move fires `drag_over` on the innermost
//! element and bubbles, and leaving fires `drag_leave`. While the drag is over it, the zone matches
//! `:state(drag-over)`, which here makes it taller. A drop ends the drag: the state clears with no
//! `drag_leave`, and the drop is dispatched as before.

mod support;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::{DragEvent, SurfaceEvent};
use zgui_view::event::events;
use zgui_view::{BuildCx, IntoView, View};

const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .zone { display: block; height: 40px }
                   .zone:state(drag-over) { height: 80px }
                   .inner { display: block; height: 20px }";

fn at(x: f32, y: f32) -> Point<CssPx, Css> {
    Point::new(CssPx(x), CssPx(y))
}

#[test]
fn a_drag_is_announced_and_styled_until_it_leaves_or_drops() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = Rc::clone(&log);
    let mut app = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let (enter, over, leave, dropped) = (
            Rc::clone(&seen),
            Rc::clone(&seen),
            Rc::clone(&seen),
            Rc::clone(&seen),
        );
        Box::new(
            zgui_elements::column()
                .child(
                    zgui_elements::r#box()
                        .class("zone")
                        .on(events::DRAG_ENTER, move |cx| {
                            enter.borrow_mut().push(format!("enter {}", cx.paths.len()));
                        })
                        .on(events::DRAG_OVER, move |_| {
                            over.borrow_mut().push("over".into())
                        })
                        .on(events::DRAG_LEAVE, move |_| {
                            leave.borrow_mut().push("leave".into())
                        })
                        .on(events::DROP, move |cx| {
                            dropped
                                .borrow_mut()
                                .push(format!("drop {}", cx.paths.len()));
                        })
                        .child(zgui_elements::r#box().class("inner")),
                )
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);
    let height = |app: &zgui_platform_headless::Harness<zgui_runtime::Runtime>| {
        app.app().windows()[0]
            .box_with_class("zone")
            .expect("the zone is laid out")
            .size
            .height
            .0
    };
    assert_eq!(height(&app), 40.0);

    let paths = vec![PathBuf::from("/tmp/a.wav"), PathBuf::from("/tmp/b.wav")];
    app.deliver_to_first(SurfaceEvent::Drag(DragEvent::Entered {
        paths: paths.clone(),
        position: at(10.0, 10.0),
    }));
    app.settle(8);
    assert_eq!(
        *log.borrow(),
        ["enter 2", "over"],
        "the inner box's drag_over bubbles"
    );
    assert_eq!(height(&app), 80.0, "the zone matches :state(drag-over)");

    app.deliver_to_first(SurfaceEvent::Drag(DragEvent::Moved {
        position: at(10.0, 200.0),
    }));
    app.settle(8);
    assert_eq!(log.borrow().last().map(String::as_str), Some("leave"));
    assert_eq!(height(&app), 40.0, "off the zone, the state clears");

    app.deliver_to_first(SurfaceEvent::Drag(DragEvent::Moved {
        position: at(10.0, 10.0),
    }));
    app.settle(8);
    log.borrow_mut().clear();
    app.deliver_to_first(SurfaceEvent::Drag(DragEvent::Dropped {
        paths,
        position: at(10.0, 10.0),
    }));
    app.settle(8);
    assert_eq!(*log.borrow(), ["drop 2"], "a drop is not also a leave");
    assert_eq!(height(&app), 40.0, "a drop ends the drag");
}
