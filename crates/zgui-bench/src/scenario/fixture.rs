//! The two documents the scenarios need that the gallery is not.
//!
//! Everything else here is measured against the shipped gallery, because a fixture written to be
//! measured measures the fixture. Two of the five scenarios ask a question the gallery cannot
//! answer, though, and both are about *size*: a thousand rows under a moving pointer, and ten
//! thousand rows scrolled past a port that only ever holds thirty of them. Neither is a synthetic
//! stand-in for a real document — the first is a table and the second is
//! [`zgui_ui::virtualize::VirtualList`], the component this library ships — but the
//! row counts are chosen rather than found, so they live apart from the gallery's own sizes.

use zgui::geom::{Css, CssPx, Point};
use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::runtime::Runtime;
use zgui::view::{Anchor, BuildCx};
use zgui::{component, view};
use zgui_ui::prelude::*;

/// How many rows the hover storm's table holds.
pub(crate) const TABLE_ROWS: usize = 1_000;

/// How many rows the still document holds.
///
/// Six elements a row, so this is the row count that puts the document at about five thousand
/// nodes — the size the idle scenario is specified at. It is a table rather than a shape invented
/// for the purpose because the question idle asks is about the *loop*, not about the document: a
/// window nothing is happening in must run no frames whatever is in it, and a document of five
/// thousand nodes is simply one large enough that a loop scanning it per turn would show.
pub(crate) const STILL_ROWS: usize = 833;

/// How tall one of its rows is, in CSS pixels, which is what a pointer is aimed by.
pub(crate) const TABLE_ROW_HEIGHT: f32 = 24.0;

/// How many rows the scroll scenario's list holds.
pub(crate) const LIST_ROWS: usize = 10_000;

/// How tall one of *those* rows is, which is what decides how many the port holds.
pub(crate) const LIST_ROW_HEIGHT: f32 = 24.0;

/// The centre of the `index`th visible row of the table, in CSS pixels.
///
/// Computed from the declared row height rather than looked up, because the point of the scenario
/// is that a pointer crossing a row costs the same whatever row it is: a lookup would walk a
/// thousand rows to find one and charge that walk to the measurement.
pub(crate) fn table_row_centre(index: usize) -> Point<CssPx, Css> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "the index is bounded by the number of rows the port shows, which is tens"
    )]
    let row = index as f32;
    Point::new(
        CssPx(crate::gallery::WIDTH / 2.0),
        CssPx(row.mul_add(TABLE_ROW_HEIGHT, TABLE_ROW_HEIGHT / 2.0)),
    )
}

/// How many of the table's rows the window shows at once.
pub(crate) fn visible_table_rows() -> usize {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the quotient of two positive window-sized lengths"
    )]
    let rows = (crate::gallery::HEIGHT / TABLE_ROW_HEIGHT) as usize;
    rows.saturating_sub(1).min(TABLE_ROWS)
}

/// What the two fixtures look like.
///
/// Deliberately plain: a row is a border, a background and five runs of text, so that what a frame
/// costs is the cost of *having* a thousand rows rather than the cost of whatever the prettiest
/// one of them does. The hover rule is the exception and is the point of the table — crossing a
/// row must restyle that row and the one left behind, and nothing else.
///
/// The list's rows carry `bench-line` instead, which is the same geometry with no hover rule. That
/// is not tidying: the pointer has to sit over a scroll container to send it a wheel event, so rows
/// slide underneath it for the whole of the scroll scenario, and a hover rule would have that
/// scenario reporting two restyles a frame that belong to the pointer rather than to scrolling.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 13px }
     .bench-table { flex-direction: column }
     .bench-row {
        flex-direction: row;
        height: 24px;
        align-items: center;
        gap: 16px;
        padding: 0 12px;
        background-color: #14161a;
        border-bottom: 1px solid #232833;
     }
     .bench-row:hover { background-color: #1e2532 }
     .bench-cell { width: 140px }
     .bench-line {
        flex-direction: row;
        height: 24px;
        align-items: center;
        gap: 16px;
        padding: 0 12px;
        background-color: #14161a;
        border-bottom: 1px solid #232833;
     }
     .bench-list { height: 1000px }"
);

/// One row of the table: five cells whose text is a function of the row's number.
#[component]
fn TableRow(
    /// Which row this is.
    index: usize,
) -> impl IntoView {
    view! {
        row(class = "bench-row") {
            text(class = "bench-cell") {{move || format!("row {index}")}}
            text(class = "bench-cell") {{move || format!("{}", index * 7 % 977)}}
            text(class = "bench-cell") {"pending"}
            text(class = "bench-cell") {{move || format!("{}.{:02}", index / 10, index % 100)}}
            text(class = "bench-cell") {"—"}
        }
    }
}

/// As many rows as asked for, all of them built.
#[component]
fn Table(
    /// How many rows to build.
    rows: usize,
) -> impl IntoView {
    view! {
        column(class = "bench-table") {
            for index in move || 0..rows, key = |index: &usize| *index {
                TableRow(index = index)
            }
        }
    }
}

/// Ten thousand rows, of which the port's worth are built.
#[component]
fn LongList() -> impl IntoView {
    let count = RwSignal::new_local(LIST_ROWS);
    view! {
        VirtualList(
            count = count,
            row_size = LIST_ROW_HEIGHT,
            label = "Bench",
            class = "bench-list",
            row = move |index: usize| view! { row(class = "bench-line") {
                text(class = "bench-cell") {{move || format!("row {index}")}}
                text(class = "bench-cell") {{move || format!("{}", index * 7 % 977)}}
            } }
        )
    }
}

/// Which fixture to build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fixture {
    /// The thousand-row table a pointer is dragged across.
    Table,
    /// The five-thousand-node document nothing at all happens in.
    Still,
    /// The ten-thousand-row virtualised list.
    LongList,
}

/// Builds the runtime holding one fixture.
///
/// # Panics
///
/// Panics when the reactive runtime's executor slot is already taken, which is a program that
/// mounted two applications on one thread rather than a measurement that failed.
pub(crate) fn runtime(fixture: Fixture) -> Runtime {
    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let app = zgui::runtime::App::new()
        .with_title("zgui-bench")
        .with_size(crate::gallery::WIDTH, crate::gallery::HEIGHT)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(crate::draw::renderer))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()));
    let built = match fixture {
        Fixture::Table => app.into_handler(|cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { Table(rows = TABLE_ROWS) }.into_view().build(cx))
        }),
        Fixture::Still => app.into_handler(|cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { Table(rows = STILL_ROWS) }.into_view().build(cx))
        }),
        Fixture::LongList => app.into_handler(|cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { LongList() }.into_view().build(cx))
        }),
    };
    built.expect("the reactive runtime installs")
}
