//! Which surface is drawn over which, and what a modal surface does to the page under it.
//!
//! # Why this is not the tree assertions beside it
//!
//! Every defect these fixtures were written for was true of a window whose document said all the
//! right things. The select's list was mounted, placed, sized and populated — and painted under the
//! dialog that opened it, because the band it went on was chosen by what kind of surface it is
//! rather than by what it was opened from. The navigation menu's panel was mounted, placed and
//! sized — and painted under the sidebar next to it, because it was positioned inside its own
//! section instead of portalled. The page behind a modal surface kept its scroll offset in the
//! scroller the whole time — and was drawn at the top, because a root that has been restyled to
//! `overflow: hidden` is not a scroll container and has no offset composed into anything.
//!
//! So nothing here reads the tree for its verdict. Each fixture opens a real window on a real
//! graphics device and asks what colour the pixels are and where the letters landed.

mod desktop;
mod device;
mod painted;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::reactive::RwSignal;
use zgui::reactive::prelude::Set;
use zgui::view;
use zgui::view::AnyView;
use zgui::vocab::NamedKey;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::{HEIGHT, SETTLED, Stage, WIDTH};
use crate::painted::words::{aim, assert_absent, assert_painted};

/// The page every fixture is laid out on.
///
/// Two of the colours are the whole of the first assertion. A dialog's panel and a select's list
/// are the same surface in this library's tokens, so a picture of a list drawn over a panel and a
/// picture of one drawn under it differ in nothing a fixture could name. Here they are told apart
/// by an element-and-class selector, which out-specifies the token that paints them both, and the
/// question "which of the two is on top" becomes a question about one pixel.
const SHEET: &str = ":root {
                         background-color: #ffffff;
                         color: #101010;
                         font-family: sans-serif;
                         overflow: auto;
                     }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }
                     box.zui-dialog { background-color: #1b3f8f; color: #ffffff }
                     box.zui-select__list { background-color: #f2c200; color: #101010 }
                     .filler { height: 220px; width: 320px }
                     .stack { height: 260px; width: 460px }
                     .card {
                         width: 460px;
                         height: 56px;
                         padding: 12px;
                         overflow: hidden;
                         background-color: #e6e6e6;
                     }";

/// The dialog panel's colour, as the sheet above paints it.
const PANEL: (u8, u8, u8) = (0x1b, 0x3f, 0x8f);

/// The select list's colour.
const LIST: (u8, u8, u8) = (0xf2, 0xc2, 0x00);

/// Opens `view`, or reports the run skipped on a machine with no graphics device.
macro_rules! staged {
    ($view:expr) => {
        match Stage::open(SHEET, $view) {
            Some(stage) => stage,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// The whole surface, which is what a picture of the window is taken over.
fn surface() -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(WIDTH), DevicePx(HEIGHT)),
    )
}

/// Where the smallest laid-out thing saying `text` is.
///
/// The smallest, for the reason [`painted::words`](crate::painted::words) gives: several nodes
/// share one label, and the outermost of them is the overlay band, which is the size of the window.
///
/// # Panics
///
/// Panics when nothing says it, because every assertion below is about to read a rectangle and one
/// taken from nowhere would agree with anything.
fn rect_saying(stage: &Stage, text: &str) -> Rect<DevicePx, Device> {
    stage
        .census()
        .nodes
        .iter()
        .filter(|node| node.text == text && node.area() > 0.0)
        .filter_map(|node| node.rect)
        .min_by(|left, right| {
            let area = |rect: Rect<DevicePx, Device>| rect.size.width.0 * rect.size.height.0;
            area(*left).total_cmp(&area(*right))
        })
        .unwrap_or_else(|| panic!("nothing laid out says {text:?}"))
}

/// The colour most of `rect` is made of, in the last frame the device drew.
///
/// # Panics
///
/// Panics for an empty rectangle, which is a reading of nothing at all.
fn dominant(stage: &Stage, rect: Rect<DevicePx, Device>) -> (u8, u8, u8) {
    let colours = stage.colours_in(rect);
    assert!(!colours.is_empty(), "no pixels inside {rect:?}");
    let mut counts: rustc_hash::FxHashMap<(u8, u8, u8), u32> = rustc_hash::FxHashMap::default();
    for colour in colours {
        *counts.entry(colour).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(colour, _)| colour)
        .expect("a rectangle with pixels in it has a commonest colour")
}

/// Whether two colours are the same to within the blending a rounded corner does.
fn alike(left: (u8, u8, u8), right: (u8, u8, u8)) -> bool {
    let near = |a: u8, b: u8| i32::from(a).abs_diff(i32::from(b)) <= 8;
    near(left.0, right.0) && near(left.1, right.1) && near(left.2, right.2)
}

// ---- a surface opened from inside another surface ----------------------------------------------

#[test]
fn a_select_opened_inside_a_dialog_paints_its_list_over_the_dialog() {
    // The list drops over the panel that opened it, which is the arrangement the defect needs: a
    // filler tall enough that the dialog extends well below the trigger, so the list has the
    // dialog's own panel behind it rather than the page.
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Dialog(default_open = true) {
                    DialogContent {
                        DialogTitle {"Preferences"}
                        Select(default_value = "gbp") {
                            SelectTrigger(a11y:label = "Currency") {
                                SelectValue(placeholder = "Choose one")
                            }
                            SelectContent {
                                SelectItem(value = "gbp") {"Pound sterling"}
                                SelectItem(value = "eur") {"Euro"}
                                SelectItem(value = "usd") {"US dollar"}
                            }
                        }
                        box(class = "filler")
                    }
                }
            }
        }
    }));
    stage.wait(SETTLED);
    assert_painted(&stage, "Preferences");

    let trigger = rect_saying(&stage, "Pound sterling");
    let below = Rect::new(
        Point::new(
            trigger.origin.x,
            DevicePx(trigger.origin.y.0 + trigger.size.height.0 + 24.0),
        ),
        Size::new(DevicePx(40.0), DevicePx(40.0)),
    );
    assert!(
        alike(dominant(&stage, below), PANEL),
        "the list is about to open over the page rather than over the dialog, so this fixture \
         would pass whichever surface won"
    );

    stage.click(aim(&stage, "Pound sterling"));
    stage.wait(SETTLED);
    stage.capture("layering-select-in-dialog");

    for option in ["Euro", "US dollar"] {
        assert_painted(&stage, option);
        let seen = dominant(&stage, rect_saying(&stage, option));
        assert!(
            alike(seen, LIST),
            "{option} is over the dialog's own panel rather than over the list: the pixels around \
             it are {seen:?}, and the list is painted {LIST:?}"
        );
    }
}

// ---- a navigation menu's panel and whatever is written next to it -------------------------------

#[test]
fn a_navigation_menu_paints_every_link_over_the_sidebar_written_under_it() {
    // The bar sits in a card and a sidebar is written under it, which is what a page with both
    // actually looks like. A panel positioned inside its own section belongs to that section, so
    // it is cut off at the card's edge and covered by whatever comes after it; a portalled one is
    // on an overlay band, above the page and outside every clip in it.
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                box(class = "card") {
                NavigationMenu(label = "Main") {
                    NavigationMenuList {
                        NavigationMenuItem(value = "products") {
                            NavigationMenuTrigger {"Products"}
                            NavigationMenuContent {
                                NavigationMenuLink {"Editor"}
                                NavigationMenuLink {"Runtime"}
                                NavigationMenuLink {"Renderer"}
                            }
                        }
                        NavigationMenuItem(value = "company") {
                            NavigationMenuTrigger {"Company"}
                            NavigationMenuContent {
                                NavigationMenuLink {"About"}
                                NavigationMenuLink {"Careers"}
                            }
                        }
                        NavigationMenuItem(value = "pricing") {
                            NavigationMenuLink {"Pricing"}
                        }
                    }
                }
                }
                box(class = "stack") {
                    SidebarProvider {
                        Sidebar(label = "Project") {
                            SidebarContent {
                                SidebarMenu {
                                    SidebarMenuItem {
                                        SidebarMenuButton {"Files"}
                                    }
                                }
                            }
                        }
                        SidebarInset {text {"The document."}}
                    }
                }
            }
        }
    }));
    stage.wait(SETTLED);

    stage.click(aim(&stage, "Products"));
    stage.wait(SETTLED);
    stage.capture("layering-navigation-over-sidebar");

    for link in ["Editor", "Runtime", "Renderer"] {
        assert_painted(&stage, link);
    }

    let card = rect_saying(&stage, "Products");
    let last = rect_saying(&stage, "Renderer");
    let sidebar = rect_saying(&stage, "Files");
    assert!(
        last.origin.y.0 > card.origin.y.0 + 44.0,
        "the panel never left the card it was written in, so nothing was ever in the way of it"
    );
    assert!(
        last.origin.y.0 + last.size.height.0 > sidebar.origin.y.0,
        "the last link stops above the sidebar, so nothing was ever in front of anything"
    );

    // The bar's other sections are closed, and a closed panel is a portalled box sitting over the
    // page: it must not be in the way of anything. Opening the next section is the shortest proof
    // that a press still reaches the bar itself.
    stage.click(aim(&stage, "Company"));
    stage.wait(SETTLED);
    for link in ["About", "Careers"] {
        assert_painted(&stage, link);
    }
    assert_absent(&stage, "Renderer");
}

// ---- a modal surface and the page under it ------------------------------------------------------

/// Where a fixture leaves the signal it drives its dialog with.
///
/// A dialog opened by pressing a button is a dialog that gives the focus back to that button when
/// it closes, and a control that has just been focused draws a ring. This fixture is a claim that
/// the window's pixels are *identical* across the whole business, so the button that would differ
/// by a ring is left out and the surface is opened from a signal instead.
#[derive(Clone, Default)]
struct Switch(Rc<RefCell<Option<RwSignal<bool, zgui::reactive::LocalStorage>>>>);

impl Switch {
    /// Records the signal the view built.
    fn keep(&self, open: RwSignal<bool, zgui::reactive::LocalStorage>) {
        *self.0.borrow_mut() = Some(open);
    }

    /// Opens or closes the surface.
    ///
    /// # Panics
    ///
    /// Panics before the view has been built.
    fn set(&self, open: bool) {
        self.0
            .borrow()
            .expect("the view built its signal")
            .set(open);
    }
}

#[test]
fn a_modal_surface_leaves_the_page_under_it_exactly_where_it_was() {
    let switch = Switch::default();
    let built = switch.clone();
    let mut stage = staged!(move || {
        let open = RwSignal::new_local(false);
        built.keep(open);
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    {(0..40)
                        .map(|row| view! { text {{format!("Row {row:02}")}} })
                        .collect::<Vec<_>>()}
                    Dialog(
                        open = open,
                        on_open_change = zgui::reactive::UnsyncCallback::new(
                            move |next: bool| open.set(next),
                        )
                    ) {
                        DialogContent {
                            DialogTitle {"Answer me"}
                        }
                    }
                }
            }
        })
    });

    // Into the middle of the page, which is the only place the defect is visible: a page already
    // at the top has nowhere to jump to.
    stage.move_to(Point::new(DevicePx(WIDTH / 2.0), DevicePx(HEIGHT / 2.0)));
    stage.wheel(6.0);
    stage.wait(SETTLED);
    stage.repaint();
    let before = stage.colours_in(surface());
    stage.capture("layering-scrolled-page");
    let landmark = rect_saying(&stage, "Row 00");
    assert!(
        landmark.origin.y.0 < 0.0,
        "the page did not scroll at all, so nothing below measures anything"
    );

    switch.set(true);
    stage.settle();
    stage.wait(SETTLED);
    assert_painted(&stage, "Answer me");
    stage.capture("layering-scrolled-page-under-a-modal");
    assert_eq!(
        rect_saying(&stage, "Row 00"),
        landmark,
        "the page moved when the dialog opened"
    );

    // Every way in, over the page rather than over the surface: a wheel, a trackpad's continuous
    // stream and the keys that page a document.
    stage.move_to(Point::new(DevicePx(60.0), DevicePx(HEIGHT - 40.0)));
    stage.wheel(10.0);
    stage.wheel(-10.0);
    stage.press_named(NamedKey::PageDown);
    stage.press_named(NamedKey::End);
    stage.wait(SETTLED);
    assert_eq!(
        rect_saying(&stage, "Row 00"),
        landmark,
        "the page scrolled behind the open dialog"
    );

    switch.set(false);
    stage.settle();
    stage.wait(SETTLED);
    stage.repaint();
    stage.capture("layering-scrolled-page-again");
    assert_eq!(
        rect_saying(&stage, "Row 00"),
        landmark,
        "the page moved when the dialog closed"
    );

    let after = stage.colours_in(surface());
    let differing = before
        .iter()
        .zip(after.iter())
        .filter(|(one, other)| one != other)
        .count();
    assert_eq!(
        differing,
        0,
        "{differing} of {} pixels changed across opening and closing the dialog",
        before.len()
    );
}
