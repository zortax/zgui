//! Where the page stands after a modal surface has come and gone.
//!
//! The defect this reproduces was found on the real gallery: after one open and close of the
//! Rename dialog, the window showed the page scrolled hard to the right with only its last column
//! visible. A photograph of a closed dialog cannot disagree with that — the surface is gone either
//! way — so the claim here is about the scroll offsets and the scrollable content itself: nothing
//! about opening and closing a surface may move the page or change how big it claims to be.

#[path = "../examples/gallery/app.rs"]
mod app;
#[path = "../examples/gallery/section/mod.rs"]
mod section;
#[path = "../examples/gallery/shell.rs"]
mod shell;

mod desktop;

use zgui::view;
use zgui::vocab::NamedKey;

use crate::app::GalleryProps;
use crate::desktop::stage::Stage;

/// Which gallery section makes the page wider than its window: each is mounted alone and the
/// root's scrollable width read back.
#[test]
fn which_section_overflows_sideways() {
    use crate::section::*;
    use zgui_ui::prelude::*;
    use zgui_ui_tokens::prelude::*;

    macro_rules! measure {
        ($name:literal, $section:ident) => {{
            let mut stage = Stage::open(crate::shell::SHEET, || {
                view! {
                    ThemeProvider {
                        Toaster {
                            column(class = "page") {
                                box(class = "grid") { $section() }
                            }
                        }
                    }
                }
            });
            stage.settle();
            let root = stage.handles().roots()[0];
            let position = stage.handles().host.scroll_position(root);
            eprintln!(
                "{:<12} content={:>6.0} port={:>6.0}",
                $name, position.content_size.width.0, position.scrollport.width.0
            );
        }};
    }

    // The data section again at other window widths, to tell a genuine minimum (a fixed floor
    // whatever the window) from width feeding back into itself (a floor that tracks the window).
    for width in [1000.0f32, 1400.0, 1600.0] {
        let mut stage = Stage::open(crate::shell::SHEET, || {
            view! {
                ThemeProvider {
                    Toaster {
                        column(class = "page") {
                            box(class = "grid") { Data() }
                        }
                    }
                }
            }
        });
        stage.deliver_resize(width, 900.0);
        stage.settle();
        let root = stage.handles().roots()[0];
        let position = stage.handles().host.scroll_position(root);
        eprintln!(
            "data@{width:<6} content={:>6.0} port={:>6.0}",
            position.content_size.width.0, position.scrollport.width.0
        );
    }

    measure!("navigation", Navigation);
    measure!("surfaces", Surfaces);
    measure!("data", Data);
    measure!("text", StyledText);
    measure!("svg", Svg);
    measure!("artwork", Artwork);
    measure!("atoms", Atoms);
    measure!("fields", Fields);
    measure!("choices", Choices);
    measure!("composites", Composites);
    measure!("feedback", Feedback);
    measure!("disclosure", Disclosure);
    measure!("overlays", Overlays);
    measure!("menus", Menus);
}

/// A wrapping flex must not claim its unwrapped width as scrollable content.
///
/// Three 200px items in a 320px wrapping row lay out as two lines; the page behind them holds
/// 320px of content, not 600. A page that believed the unwrapped sum would grow a horizontal
/// scrollbar over nothing, and everything aimed by revealed coordinates afterwards would land in
/// the blank.
#[test]
fn a_wrapped_flex_claims_the_width_it_wrapped_to() {
    let mut stage = Stage::open(
        ":root { background-color: #fff; color: #111; font-family: sans-serif; overflow: auto }
         .wrap { display: flex; flex-wrap: wrap; width: 320px; gap: 0px }
         .item { width: 200px; height: 40px; flex: none; background-color: #eee }
         .full { width: 100%; height: 40px; flex: none; background-color: #ddd }
         .tall { width: 50px; height: 2000px }",
        || {
            view! {
                box {
                    box(class = "wrap") {
                        box(class = "item")
                        box(class = "full")
                        box(class = "item")
                    }
                    box(class = "tall")
                }
            }
        },
    );
    stage.settle();
    let root = stage.handles().roots()[0];
    let position = stage.handles().host.scroll_position(root);
    eprintln!(
        "wrap fixture: content={:?} port={:?}",
        position.content_size, position.scrollport
    );
    assert!(
        position.content_size.width.0 <= position.scrollport.width.0 + 0.5,
        "a wrapped flex reported its unwrapped width: content {:?} in a port {:?}",
        position.content_size,
        position.scrollport
    );
}

/// A data table in a narrow box must shrink into it rather than hold a width of its own.
#[test]
fn the_data_table_fits_the_box_it_is_given() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use zgui::geom::{DevicePx, Size};
    use zgui::platform::SurfaceEvent;
    use zgui_ui::data_table::Column;
    use zgui_ui::prelude::*;

    struct TreeGrab(Rc<RefCell<String>>);
    impl zgui::runtime::FrameProbe for TreeGrab {
        fn frame_ended(&self, window: &zgui::runtime::Window) {
            *self.0.borrow_mut() = zgui_layout::tree::print::to_text(&window.layout().borrow());
        }
    }

    let text = Rc::new(RefCell::new(String::new()));
    let probe = Rc::new(TreeGrab(Rc::clone(&text)));
    let handler = zgui::app()
        .with_size(1000.0, 900.0)
        .with_stylesheet(
            ":root { background-color: #fff; color: #111; font-family: sans-serif; overflow: auto }
             .narrow { width: 600px }",
        )
        .with_renderer(Box::new(crate::desktop::renderer::build))
        .with_probe(probe)
        .into_handler(|| {
            let rows: Vec<(String, u32)> = vec![
                ("Paper".into(), 250),
                ("Ink".into(), 1250),
                ("Binding".into(), 400),
                ("Postage".into(), 190),
                ("Envelopes".into(), 320),
            ];
            let columns = vec![
                Column::new("item", "Item", |row: &(String, u32)| row.0.clone())
                    .sortable_by(|a: &(String, u32), b: &(String, u32)| a.0.cmp(&b.0)),
                Column::new("cost", "Cost", |row: &(String, u32)| format!("{}p", row.1))
                    .sized("120px"),
            ];
            zgui::view! {
                box(class = "narrow") {
                    DataTable(
                        rows = rows,
                        columns = columns,
                        row_id = |row: &(String, u32)| row.1.to_string(),
                        label = "Invoice lines",
                        selectable = true,
                        filterable = true,
                        page_size = 3_usize
                    )
                }
            }
        })
        .expect("the application builds");
    let mut inner = None;
    handler
        .drive(|app| {
            inner = Some(app);
            Ok(())
        })
        .expect("the driver takes the application");
    let mut harness =
        zgui_platform_headless::Harness::new(inner.expect("the driver was handed the app"));
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(1000.0),
        DevicePx(900.0),
    )));
    harness.settle(64);
    std::fs::write("/tmp/zgui-dt-tree.txt", text.borrow().as_str()).expect("writable");
    let widest = text
        .borrow()
        .lines()
        .filter(|line| line.contains("size=("))
        .filter_map(|line| {
            let size = line.split("size=(").nth(1)?;
            size.split(&[' ', 'x'][..])
                .next()?
                .trim()
                .parse::<f32>()
                .ok()
        })
        .fold(0.0f32, f32::max);
    assert!(
        widest <= 1000.0,
        "something inside the table is {widest} wide; the tree is at /tmp/zgui-dt-tree.txt"
    );
}

/// A right-justified row places every item inside its own edge.
///
/// The data table's pager — words, then two buttons, pushed to the right — came out with the
/// buttons a whole container-width past where the words were placed, which is the picture the
/// footer showed: a page count and no buttons. Three fixed items in a 320px end-justified row
/// belong at 110, 190 and 260.
#[test]
fn an_end_justified_row_keeps_its_items_inside() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use zgui::geom::{DevicePx, Size};
    use zgui::platform::SurfaceEvent;

    struct TreeGrab(Rc<RefCell<String>>);
    impl zgui::runtime::FrameProbe for TreeGrab {
        fn frame_ended(&self, window: &zgui::runtime::Window) {
            *self.0.borrow_mut() = zgui_layout::tree::print::to_text(&window.layout().borrow());
        }
    }

    use zgui::reactive::RwSignal;
    use zgui::reactive::prelude::{Get, Set};

    let text = Rc::new(RefCell::new(String::new()));
    let probe = Rc::new(TreeGrab(Rc::clone(&text)));
    let words: Rc<RefCell<Option<RwSignal<String, zgui::reactive::LocalStorage>>>> =
        Rc::new(RefCell::new(None));
    let handed = Rc::clone(&words);
    let handler = zgui::app()
        .with_size(1000.0, 900.0)
        .with_stylesheet(
            ":root { background-color: #fff; color: #111; font-family: sans-serif; overflow: auto }
             .row { display: flex; justify-content: flex-end; gap: 10px; width: 320px }
             .words { height: 20px; margin-right: auto }
             .item { width: 60px; height: 20px; flex: none }",
        )
        .with_renderer(Box::new(crate::desktop::renderer::build))
        .with_probe(probe)
        .into_handler(move || {
            let signal = RwSignal::new_local("Page 1 of 1".to_owned());
            *handed.borrow_mut() = Some(signal);
            zgui::view! {
                box(class = "row") {
                    box(class = "words") { text {{move || signal.get()}} }
                    box(class = "item")
                    box(class = "item")
                }
            }
        })
        .expect("the application builds");
    let mut inner = None;
    handler
        .drive(|app| {
            inner = Some(app);
            Ok(())
        })
        .expect("the driver takes the application");
    let mut harness =
        zgui_platform_headless::Harness::new(inner.expect("the driver was handed the app"));
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(1200.0),
        DevicePx(900.0),
    )));
    harness.settle(64);

    // The row's own line, read off the printed tree: the words' auto margin absorbs the free
    // space, so the words sit at the start and the fixed items pack against the end.
    let assert_positions = |tree: &str| {
        let places: Vec<f32> = tree
            .lines()
            .skip_while(|line| !line.contains("size=(320"))
            .filter(|line| line.contains("fc=block at=("))
            .take(3)
            .filter_map(|line| {
                let at = line.split("at=(").nth(1)?;
                at.split(',').next()?.trim().parse().ok()
            })
            .collect();
        assert_eq!(
            places,
            vec![0.0, 190.0, 260.0],
            "the free space the auto margin absorbed was applied again by justify-content:\n{tree}"
        );
        assert!(
            !tree.contains("size=(320 x 20) content=(4"),
            "the row claims content wider than itself:\n{tree}"
        );
    };
    assert_positions(&text.borrow());

    // The words change after the window has already laid out, which is what a data model's echo
    // does to the real pager; the row must not drift.
    words
        .borrow()
        .expect("the view was built")
        .set("Page 1 of 2".to_owned());
    harness.settle(64);
    assert_positions(&text.borrow());
}

/// The whole computed tree of the data section, for reading off where the width went.
#[test]
fn print_the_data_section_s_layout() {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::section::*;
    use zgui::geom::{DevicePx, Size};
    use zgui::platform::SurfaceEvent;
    use zgui_ui::prelude::*;
    use zgui_ui_tokens::prelude::*;

    struct TreeGrab(Rc<RefCell<String>>);
    impl zgui::runtime::FrameProbe for TreeGrab {
        fn frame_ended(&self, window: &zgui::runtime::Window) {
            *self.0.borrow_mut() = zgui_layout::tree::print::to_text(&window.layout().borrow());
        }
    }

    let text = Rc::new(RefCell::new(String::new()));
    let probe = Rc::new(TreeGrab(Rc::clone(&text)));
    let handler = zgui::app()
        .with_size(1000.0, 900.0)
        .with_stylesheet(crate::shell::SHEET)
        .with_renderer(Box::new(crate::desktop::renderer::build))
        .with_probe(probe)
        .into_handler(|| {
            zgui::view! {
                ThemeProvider {
                    Toaster {
                        column(class = "page") {
                            box(class = "grid") { Data() }
                        }
                    }
                }
            }
        })
        .expect("the application builds");
    let mut inner = None;
    handler
        .drive(|app| {
            inner = Some(app);
            Ok(())
        })
        .expect("the driver takes the application");
    let mut harness =
        zgui_platform_headless::Harness::new(inner.expect("the driver was handed the app"));
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(1000.0),
        DevicePx(900.0),
    )));
    harness.settle(64);
    let held = text.borrow();
    eprintln!("tree: {} lines", held.lines().count());
    std::fs::write("/tmp/zgui-data-tree.txt", held.as_str()).expect("the dump is writable");
}

/// Which box inside the data section reaches past the window.
#[test]
fn what_the_data_section_puts_past_the_edge() {
    use crate::section::*;
    use zgui_ui::prelude::*;
    use zgui_ui_tokens::prelude::*;

    let mut stage = Stage::open(crate::shell::SHEET, || {
        view! {
            ThemeProvider {
                Toaster {
                    column(class = "page") {
                        box(class = "grid") { Data() }
                    }
                }
            }
        }
    });
    stage.settle();
    let census = stage.census();
    for seen in &census.nodes {
        let Some(rect) = seen.rect else { continue };
        if rect.origin.x.0 + rect.size.width.0 > 1250.0 {
            eprintln!(
                "past: {:>6.0}..{:<6.0} w={:<6.0} depth={} {:?}",
                rect.origin.x.0,
                rect.origin.x.0 + rect.size.width.0,
                rect.size.width.0,
                seen.depth,
                seen.text.chars().take(44).collect::<String>()
            );
        }
    }
}

#[test]
fn a_dialog_cycle_leaves_the_page_where_it_stood() {
    let mut stage = Stage::open(crate::shell::SHEET, || view! { app::Gallery() });
    stage.settle();

    let root = stage.handles().roots()[0];

    // The page is scrolled part-way down first, the way a person reaches the dialog's own panel:
    // the defect only showed on a page that was somewhere, not at its origin.
    for _ in 0..12 {
        stage.wheel(10.0);
    }
    stage.settle();

    let before = stage.handles().host.scroll_position(root);
    eprintln!(
        "before: offset={:?} content={:?} port={:?}",
        before.offset, before.content_size, before.scrollport
    );

    // Which boxes are wider than the port: the page's scrollable width is the widest of these,
    // and the widest is the one to go and look at.
    let census = stage.census();
    for seen in &census.nodes {
        let Some(rect) = seen.rect else { continue };
        if rect.size.width.0 > 1300.0 {
            eprintln!(
                "wide: {:>6.0}..{:<6.0} w={:<6.0} depth={} {:?}",
                rect.origin.x.0,
                rect.origin.x.0 + rect.size.width.0,
                rect.size.width.0,
                seen.depth,
                seen.text.chars().take(48).collect::<String>()
            );
        }
    }

    stage.click_saying("Rename…");
    stage.settle();
    let open = stage.handles().host.scroll_position(root);
    eprintln!(
        "open:   offset={:?} content={:?} port={:?}",
        open.offset, open.content_size, open.scrollport
    );

    stage.key(NamedKey::Escape);
    stage.settle();
    let after = stage.handles().host.scroll_position(root);
    eprintln!(
        "after:  offset={:?} content={:?} port={:?}",
        after.offset, after.content_size, after.scrollport
    );

    assert_eq!(
        before.offset, after.offset,
        "opening and closing a dialog moved the page"
    );
    assert_eq!(
        before.content_size, after.content_size,
        "opening and closing a dialog changed what the page claims to hold"
    );
    assert_eq!(
        before.offset, open.offset,
        "opening a dialog moved the page underneath it"
    );
    assert_eq!(
        before.content_size, open.content_size,
        "an open dialog grew the page behind it"
    );
}

/// A row of overlapping discs — an avatar stack — measures the width it actually occupies.
///
/// Each member is unshrinkable and laps the one before it by a negative margin, and that pairing is
/// what the flex container's intrinsic main size used to be computed wrongly for: the overlap came
/// back multiplied by the member's whole width, so the container measured at nothing and every
/// member piled up on the first. See the `scaled_shrink_factor` note in `vendor/taffy`.
#[test]
fn an_overlapping_stack_measures_its_whole_width() {
    let mut stage = Stage::open(
        ":root { background-color: #fff; color: #111; font-family: sans-serif }
         .row { display: flex; flex-direction: row; flex-wrap: wrap; gap: 8px }
         .stack { display: flex; flex-direction: row }
         .face { width: 32px; height: 32px; flex: none; background-color: #ccc }
         .stack > .face:not(:first-child) { margin-left: -8px }",
        || {
            view! {
                box(class = "row") {
                    box(class = "stack") {
                        box(class = "face")
                        box(class = "face")
                        box(class = "face")
                        box(class = "face")
                    }
                    box(class = "stack") {
                        box(class = "face")
                        box(class = "face")
                        box(class = "face")
                    }
                }
            }
        },
    );
    stage.settle();

    let stacks: Vec<_> = stage
        .census()
        .nodes
        .iter()
        .filter_map(|node| node.rect)
        .filter(|rect| (rect.size.height.0 - 32.0).abs() < 0.5 && rect.size.width.0 > 32.5)
        .map(|rect| (rect.origin.x.0, rect.size.width.0))
        .collect();

    // Four faces lapping by eight is 4 × 32 − 3 × 8, and three of them is 3 × 32 − 2 × 8.
    assert!(
        stacks.iter().any(|(_, width)| (width - 104.0).abs() < 0.5),
        "the four-deep stack measured {stacks:?} rather than 104 wide"
    );
    assert!(
        stacks.iter().any(|(_, width)| (width - 80.0).abs() < 0.5),
        "the three-deep stack measured {stacks:?} rather than 80 wide"
    );
    // And the two of them stand beside each other rather than one inside the other.
    let (first, second) = (
        stacks
            .iter()
            .find(|(_, width)| (width - 104.0).abs() < 0.5)
            .expect("the four-deep stack"),
        stacks
            .iter()
            .find(|(_, width)| (width - 80.0).abs() < 0.5)
            .expect("the three-deep stack"),
    );
    assert!(
        second.0 >= first.0 + first.1,
        "the stacks overlap: one at {first:?} and the next at {second:?}"
    );
}
