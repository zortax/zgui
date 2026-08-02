//! The data surfaces, driven: scrolled, sorted, chosen from, paged, walked and read back.
//!
//! Every test here builds a real component through the ordinary view path, drives it with a real
//! event or a real observation, and then asks the tree what changed. Nothing hand-builds the input
//! a component is supposed to compute for itself, and nothing asserts that a view compiles.

mod harness;

use std::rc::Rc;

use zgui::geom::{DevicePx, Point, Size};
use zgui::prelude::*;
use zgui::reactive::{RwSignal, UnsyncCallback};
use zgui::view;
use zgui::view::{CustomPropertyName, ObservedValue, PropKey, ScrollPosition};
use zgui::vocab::{NamedKey, Role, SortDirection};
use zgui_ui::prelude::*;

use crate::harness::Harness;

/// How tall the scrollport is in every virtualisation test, in device pixels.
const PORT: f32 = 400.0;

/// How tall one row is, in CSS pixels and — at a scale of one — in device pixels too.
const ROW: f32 = 20.0;

/// The scroll position a port `PORT` tall, scrolled to `offset`, over `rows` rows, reports.
fn scrolled(offset: f32, rows: usize) -> ScrollPosition {
    ScrollPosition {
        offset: Point::new(DevicePx(0.0), DevicePx(offset)),
        content_size: Size::new(DevicePx(300.0), DevicePx(rows as f32 * ROW)),
        scrollport: Size::new(DevicePx(300.0), DevicePx(PORT)),
    }
}

/// Every element under the root carrying `class`.
fn all_with(harness: &Harness, class: &str) -> Vec<NodeId> {
    let name = zgui::view::ClassName::new(class);
    harness
        .all()
        .into_iter()
        .filter(|node| harness.window.dom.tree().classes(*node).contains(&name))
        .collect()
}

/// Sends one pointer event straight at an element, at a place in CSS pixels.
fn point_at(harness: &Harness, node: NodeId, kind: zgui::vocab::EventKind, x: f32, y: f32) {
    harness.window.dispatcher().send_to(
        node,
        kind,
        zgui::vocab::Payload::Pointer(zgui::vocab::PointerEvent::mouse(Point::new(
            zgui::geom::CssPx(x),
            zgui::geom::CssPx(y),
        ))),
    );
    harness.window.frame();
}

/// The `data-index` attribute of every element carrying `class`, in tree order.
fn indices_of(harness: &Harness, class: &str) -> Vec<usize> {
    all_with(harness, class)
        .into_iter()
        .filter_map(|node| harness.attribute(node, "data-index"))
        .filter_map(|value| value.parse().ok())
        .collect()
}

// ---- virtualisation -----------------------------------------------------------------------------

/// A list of `rows` rows, mounted with its scrollport declared and one frame run.
fn virtual_list(harness: &Harness, rows: usize) -> NodeId {
    let port = NodeRef::new();
    let count = harness.window.scope.with(|| RwSignal::new_local(rows));
    let handle = harness.window.scope.with(|| port);
    harness.mount(move || {
        view! {
            VirtualList(
                count = count,
                row_size = ROW,
                overscan = 2_usize,
                label = "Rows",
                node_ref = handle,
                row = move |index: usize| view! { text {{move || format!("row {index}")}} }
            )
        }
    });
    let scroll = harness.only_child();
    harness
        .window
        .dom
        .deliver(scroll, ObservedValue::ScrollPosition(scrolled(0.0, rows)));
    harness.window.frame();
    scroll
}

#[test]
fn ten_thousand_rows_mount_a_bounded_number_of_elements() {
    let harness = Harness::open();
    let scroll = virtual_list(&harness, 10_000);

    let rows = indices_of(&harness, "zui-virtual-list__row");
    assert_eq!(
        rows.len(),
        23,
        "twenty-one rows fit the port, plus two of overscan — not ten thousand",
    );
    assert_eq!(rows.first().copied(), Some(0));

    // The real claim, and the one an arbitrary threshold could not make: the number of elements is
    // a function of the port, not of the data. A hundred rows and ten thousand cost the same.
    let small = Harness::open();
    virtual_list(&small, 100);
    assert_eq!(
        harness.all().len(),
        small.all().len(),
        "a ten-thousand-row list cost more than a hundred-row one",
    );

    // The list still measures the whole ten thousand: the two spacers stand in for the rest.
    let pane = harness.find("zui-virtual-list__pane");
    let trail = harness
        .window
        .dom
        .tree()
        .custom_property(pane, CustomPropertyName::new("zui-virtual-trail"))
        .expect("the trailing spacer is written");
    assert_eq!(trail, format!("{}px", (10_000 - 23) as f32 * ROW));
    let _ = scroll;
}

#[test]
fn scrolling_less_than_one_row_does_not_touch_the_tree_at_all() {
    let harness = Harness::open();
    let scroll = virtual_list(&harness, 10_000);
    let before = harness.all().len();

    harness.window.transcript.clear();
    harness.window.dom.deliver(
        scroll,
        ObservedValue::ScrollPosition(scrolled(ROW - 1.0, 10_000)),
    );
    harness.window.frame();

    assert_eq!(
        harness.window.transcript.to_string(),
        "",
        "a sub-row scroll rebuilt something; off-screen rows are being re-entered",
    );
    assert_eq!(harness.all().len(), before);
}

#[test]
fn scrolling_by_one_row_builds_one_row_and_destroys_one() {
    let harness = Harness::open();
    let scroll = virtual_list(&harness, 10_000);
    // Well clear of the start, where neither edge of the window is clamped.
    harness.window.dom.deliver(
        scroll,
        ObservedValue::ScrollPosition(scrolled(ROW * 500.0, 10_000)),
    );
    harness.window.frame();

    let before = indices_of(&harness, "zui-virtual-list__row");
    let count = harness.all().len();
    let per_row = count / before.len().max(1);

    harness.window.transcript.clear();
    harness.window.dom.deliver(
        scroll,
        ObservedValue::ScrollPosition(scrolled(ROW * 501.0, 10_000)),
    );
    harness.window.frame();

    let after = indices_of(&harness, "zui-virtual-list__row");
    assert_eq!(
        after.first().copied(),
        Some(before[0] + 1),
        "the window moved by one row"
    );
    assert_eq!(after.len(), before.len(), "and stayed the same size");
    assert_eq!(harness.all().len(), count, "so the tree did too");

    // The rows that did not move were not rebuilt: a keyed list keeps them, and a windowed list
    // that rebuilt its whole body on every scroll would be a list that virtualises nothing.
    let transcript = harness.window.transcript.to_string();
    let created = transcript.matches("create").count();
    assert!(
        created <= per_row + 1,
        "one row moved and {created} nodes were created (a row is {per_row}):\n{transcript}",
    );
    assert!(
        transcript.matches("remove").count() <= per_row + 1,
        "one row moved and more than one was taken out:\n{transcript}",
    );
}

#[test]
fn a_thousand_scroll_steps_leave_the_owner_tree_and_the_element_count_where_they_started() {
    // The soak: a virtualised list is the only thing in this library that churns reactive scopes
    // at a rate that can expose a leak, and a leak here is invisible until an application has been
    // scrolling for an hour.
    let harness = Harness::open();
    let scroll = virtual_list(&harness, 10_000);

    let elements = harness.all().len();
    let depth = harness.window.scope.with(|| {
        zgui::reactive::Owner::current()
            .expect("a scope")
            .ancestry()
            .len()
    });

    for step in 0..1_000 {
        let offset = (step as f32 * 137.0) % ((10_000 - 25) as f32 * ROW);
        harness.window.dom.deliver(
            scroll,
            ObservedValue::ScrollPosition(scrolled(offset, 10_000)),
        );
        harness.window.frame();
    }
    // Back where it started, so the comparison is of the same window rather than of two.
    harness
        .window
        .dom
        .deliver(scroll, ObservedValue::ScrollPosition(scrolled(0.0, 10_000)));
    harness.window.frame();

    assert_eq!(
        harness.all().len(),
        elements,
        "the tree grew over a thousand scroll steps",
    );
    assert_eq!(
        harness
            .window
            .scope
            .with(|| zgui::reactive::Owner::current()
                .expect("a scope")
                .ancestry()
                .len()),
        depth,
        "the owner chain grew over a thousand scroll steps",
    );

    // Every surviving row still answers, rather than reading through a scope that was disposed of.
    let rows = indices_of(&harness, "zui-virtual-list__row");
    assert!(!rows.is_empty());
    for node in all_with(&harness, "zui-virtual-list__row") {
        let semantics = harness.semantics(node);
        assert_eq!(semantics.role, Role::ListItem);
        assert_eq!(
            semantics.position.size_of_set,
            Some(10_000),
            "a row of a windowed list must report the length of the whole list",
        );
    }
}

// ---- the table ---------------------------------------------------------------------------------

/// One row of the fixture.
#[derive(Clone, PartialEq, Debug)]
struct Row {
    /// What identifies it.
    id: u32,
    /// A word.
    name: String,
    /// A number.
    size: i64,
}

/// `count` rows, named backwards so that sorting has something to do.
fn rows(count: u32) -> Vec<Row> {
    (0..count)
        .map(|id| Row {
            id,
            name: format!("row-{:04}", count - id - 1),
            size: i64::from(count - id) * 10,
        })
        .collect()
}

/// The two sortable columns and one that is not.
fn columns() -> Vec<Column<Row>> {
    vec![
        Column::new("name", "Name", |row: &Row| row.name.clone())
            .sortable_by(|left: &Row, right: &Row| left.name.cmp(&right.name)),
        Column::new("size", "Size", |row: &Row| row.size.to_string())
            .aligned(CellAlign::End)
            .sized("120px")
            .sortable_by(|left: &Row, right: &Row| left.size.cmp(&right.size)),
    ]
}

/// The text of every cell in the first column, in tree order.
fn first_column(harness: &Harness) -> Vec<String> {
    all_with(harness, "zui-table__cell")
        .into_iter()
        .filter(|node| harness.attribute(*node, "data-column").as_deref() == Some("0"))
        .map(|node| harness.window.dom.tree().text_content(node))
        .collect()
}

#[test]
fn pressing_a_header_sorts_the_rows_that_are_actually_in_the_tree() {
    let harness = Harness::open();
    let source = harness.window.scope.with(|| RwSignal::new_local(rows(6)));
    harness.mount(move || {
        view! {
            DataTable(
                rows = source,
                columns = columns(),
                row_id = |row: &Row| row.id.to_string(),
                label = "Rows"
            )
        }
    });

    let before = first_column(&harness);
    assert_eq!(before.first().map(String::as_str), Some("row-0005"));

    let header = all_with(&harness, "zui-data-table__sort")[0];
    harness.click(header);

    let after = first_column(&harness);
    assert_eq!(
        after.first().map(String::as_str),
        Some("row-0000"),
        "a press on the header did not reorder the rows in the tree",
    );
    assert_ne!(before, after);

    // And the header says so, to a reader as well as to a sheet.
    let cell = all_with(&harness, "zui-table__head")[0];
    assert_eq!(
        harness.attribute(cell, "data-sort").as_deref(),
        Some("ascending")
    );
    assert_eq!(
        harness.semantics(cell).sort_direction,
        Some(SortDirection::Ascending),
        "a column header that sorts must say which way, or a reader cannot tell",
    );

    harness.click(header);
    assert_eq!(
        first_column(&harness).first().map(String::as_str),
        Some("row-0005"),
        "a second press reverses it",
    );
    assert_eq!(
        harness.semantics(cell).sort_direction,
        Some(SortDirection::Descending)
    );
}

#[test]
fn a_row_is_chosen_by_pressing_its_box_and_stays_chosen_when_the_table_is_sorted() {
    let harness = Harness::open();
    let source = harness.window.scope.with(|| RwSignal::new_local(rows(6)));
    harness.mount(move || {
        view! {
            DataTable(
                rows = source,
                columns = columns(),
                row_id = |row: &Row| row.id.to_string(),
                label = "Rows",
                selectable = true
            )
        }
    });

    // The first body checkbox, which is the one in the first row rather than the one in the header.
    let boxes = all_with(&harness, "zui-checkbox");
    assert_eq!(boxes.len(), 7, "one per row, and one in the header");
    harness.click(boxes[1]);

    let row = all_with(&harness, "zui-table__row")[1];
    assert_eq!(
        harness.attribute(row, "data-selected").as_deref(),
        Some("true")
    );
    assert_eq!(
        harness.semantics(row).selected,
        Some(true),
        "a chosen row has to say so to a reader, not only to a sheet",
    );

    // The header now stands for "some", not "all" and not "none".
    assert_eq!(
        harness.attribute(boxes[0], "data-state").as_deref(),
        Some("indeterminate"),
    );

    // Sorting moves the row; the choice follows the row rather than the position.
    let header = all_with(&harness, "zui-data-table__sort")[0];
    harness.click(header);
    let chosen: Vec<String> = all_with(&harness, "zui-table__row")
        .into_iter()
        .filter(|node| harness.attribute(*node, "data-selected").as_deref() == Some("true"))
        .map(|node| harness.window.dom.tree().text_content(node))
        .collect();
    assert_eq!(chosen.len(), 1, "exactly one row is still chosen");
    assert!(
        chosen[0].contains("row-0005"),
        "the choice moved to a different row: {chosen:?}",
    );
}

#[test]
fn selecting_everything_from_the_header_chooses_every_row() {
    let harness = Harness::open();
    let source = harness.window.scope.with(|| RwSignal::new_local(rows(6)));
    harness.mount(move || {
        view! {
            DataTable(
                rows = source,
                columns = columns(),
                row_id = |row: &Row| row.id.to_string(),
                label = "Rows",
                selectable = true
            )
        }
    });

    harness.click(all_with(&harness, "zui-checkbox")[0]);
    let selected = all_with(&harness, "zui-table__row")
        .into_iter()
        .filter(|node| harness.attribute(*node, "data-selected").as_deref() == Some("true"))
        .count();
    assert_eq!(selected, 6);
}

#[test]
fn the_pager_moves_between_pages_and_says_where_it_is() {
    let harness = Harness::open();
    let source = harness.window.scope.with(|| RwSignal::new_local(rows(25)));
    harness.mount(move || {
        view! {
            DataTable(
                rows = source,
                columns = columns(),
                row_id = |row: &Row| row.id.to_string(),
                label = "Rows",
                page_size = 10_usize
            )
        }
    });

    assert_eq!(first_column(&harness).len(), 10);
    let page = harness.find("zui-data-table__page");
    assert_eq!(harness.window.dom.tree().text_content(page), "Page 1 of 3");

    // The pager's buttons are the table's own, and the table's own sheet styles them. A component
    // that borrowed another component's class would be unstyled on every page that does not happen
    // to have that other component on it.
    let steps = all_with(&harness, "zui-data-table__step");
    assert_eq!(steps.len(), 2, "a step back and a step on");
    assert!(
        zgui_ui::data_table::DataTableStyle::CSS.contains(".zui-data-table__step"),
        "the pager's buttons are styled by a sheet the table does not install",
    );
    assert!(
        all_with(&harness, "zui-calendar__step").is_empty(),
        "the pager is wearing the calendar's class",
    );

    harness.click(steps[1]);
    assert_eq!(harness.window.dom.tree().text_content(page), "Page 2 of 3");
    assert_eq!(
        first_column(&harness).first().map(String::as_str),
        Some("row-0014"),
        "the second page shows the second ten rows",
    );

    harness.click(steps[0]);
    assert_eq!(
        harness.window.dom.tree().text_content(page),
        "Page 1 of 3",
        "and the step back goes back",
    );
}

#[test]
fn dragging_a_grip_resizes_the_column_it_belongs_to_and_never_below_the_floor() {
    let harness = Harness::open();
    let source = harness.window.scope.with(|| RwSignal::new_local(rows(3)));
    harness.mount(move || {
        view! {
            DataTable(
                rows = source,
                columns = columns(),
                row_id = |row: &Row| row.id.to_string(),
                label = "Rows"
            )
        }
    });

    let table = harness.find("zui-table");
    let tracks = CustomPropertyName::new("zui-table-columns");
    let header = all_with(&harness, "zui-table__head")[1];
    harness.window.place(header, 0.0, 0.0, 120.0, 32.0);
    let grip = all_with(&harness, "zui-data-table__grip")[1];

    // A press, a move and a release — the three events a drag is, at the grip itself.
    point_at(
        &harness,
        grip,
        zgui::vocab::EventKind::PointerDown,
        120.0,
        16.0,
    );
    point_at(
        &harness,
        grip,
        zgui::vocab::EventKind::PointerMove,
        170.0,
        16.0,
    );
    assert_eq!(
        harness.window.dom.tree().custom_property(table, tracks),
        Some(String::from("1fr 170px")),
        "the drag moved the separator and the track list did not follow",
    );

    // Dragged back past nothing at all: a column nobody can find again is not a column.
    point_at(
        &harness,
        grip,
        zgui::vocab::EventKind::PointerMove,
        -400.0,
        16.0,
    );
    assert_eq!(
        harness.window.dom.tree().custom_property(table, tracks),
        Some(format!("1fr {}px", zgui_ui::data_table::MIN_WIDTH)),
    );

    point_at(&harness, grip, zgui::vocab::EventKind::PointerUp, 0.0, 16.0);
    let after_release = harness.window.dom.tree().custom_property(table, tracks);
    point_at(
        &harness,
        grip,
        zgui::vocab::EventKind::PointerMove,
        900.0,
        16.0,
    );
    assert_eq!(
        harness.window.dom.tree().custom_property(table, tracks),
        after_release,
        "the pointer is no longer down, so moving it must not still resize",
    );
}

#[test]
fn a_virtualised_table_states_the_shape_its_elements_do_not_have() {
    let harness = Harness::open();
    let source = harness
        .window
        .scope
        .with(|| RwSignal::new_local(rows(10_000)));
    let grid = harness.window.scope.with(NodeRef::new);
    harness.mount(move || {
        view! {
            DataTable(
                rows = source,
                columns = columns(),
                row_id = |row: &Row| row.id.to_string(),
                label = "Rows",
                virtualized = true,
                row_size = ROW,
                node_ref = grid
            )
        }
    });

    let table = harness.find("zui-table");
    harness
        .window
        .dom
        .deliver(table, ObservedValue::ScrollPosition(scrolled(0.0, 10_000)));
    harness.window.frame();

    let built = all_with(&harness, "zui-table__row").len();
    assert!(
        built < 40,
        "a ten-thousand-row table built {built} rows in the tree",
    );

    let semantics = harness.semantics(table);
    assert_eq!(semantics.role, Role::Table);
    assert_eq!(
        semantics.table.row_count,
        Some(10_000),
        "counting the rows in the tree would announce a table of thirty",
    );
    assert_eq!(semantics.table.column_count, Some(2));

    // And each row says where it really is, for the same reason: a reader counting the rows in the
    // tree would be counting a window.
    let numbered: Vec<usize> = all_with(&harness, "zui-table__row")
        .into_iter()
        .filter_map(|node| harness.semantics(node).table.row_index)
        .collect();
    assert_eq!(numbered.first().copied(), Some(0));
    assert_eq!(
        numbered.len(),
        built - 1,
        "every body row but the header carries one"
    );
}

#[test]
fn a_columns_width_is_changed_from_the_keyboard_and_reaches_the_track_list() {
    let harness = Harness::open();
    let source = harness.window.scope.with(|| RwSignal::new_local(rows(3)));
    harness.mount(move || {
        view! {
            DataTable(
                rows = source,
                columns = columns(),
                row_id = |row: &Row| row.id.to_string(),
                label = "Rows"
            )
        }
    });

    let table = harness.find("zui-table");
    let tracks = CustomPropertyName::new("zui-table-columns");
    assert_eq!(
        harness.window.dom.tree().custom_property(table, tracks),
        Some(String::from("1fr 120px")),
    );

    // The grip has to know how wide its header is, and it asks the engine — so the test declares
    // the header's box exactly as a laid-out frame would.
    let header = all_with(&harness, "zui-table__head")[1];
    harness.window.place(header, 0.0, 0.0, 120.0, 32.0);

    let grip = all_with(&harness, "zui-data-table__grip")[1];
    harness.press(grip, NamedKey::ArrowRight);

    assert_eq!(
        harness.window.dom.tree().custom_property(table, tracks),
        Some(String::from("1fr 128px")),
        "the arrow key moved the separator and the track list did not follow",
    );
}

// ---- the calendar ------------------------------------------------------------------------------

/// A calendar showing July 2026, and its grid element.
fn calendar(harness: &Harness) -> NodeId {
    let start = Date::new(2026, 7, 15).expect("a real date");
    harness.mount(move || {
        view! { Calendar(default_value = start, label = "Arrival") }
    });
    harness.find("zui-calendar__grid")
}

/// The `data-date` of the day that holds the grid's single tab stop.
fn tab_stop(harness: &Harness) -> Option<String> {
    all_with(harness, "zui-calendar__day")
        .into_iter()
        .find(|node| harness.attribute(*node, "tabindex").as_deref() == Some("0"))
        .and_then(|node| harness.attribute(node, "data-date"))
}

#[test]
fn a_calendar_is_a_grid_of_weeks_and_days_a_reader_can_name() {
    let harness = Harness::open();
    let grid = calendar(&harness);

    assert_eq!(harness.semantics(grid).role, Role::Grid);
    let days = all_with(&harness, "zui-calendar__day");
    assert_eq!(days.len(), 42, "six weeks of seven days, always");

    let weekdays = all_with(&harness, "zui-calendar__weekday");
    assert_eq!(weekdays.len(), 7);
    assert_eq!(harness.semantics(weekdays[0]).role, Role::ColumnHeader);

    let rows = all_with(&harness, "zui-calendar__week");
    assert_eq!(rows.len(), 7, "six weeks and the strip of weekday names");
    assert!(
        rows.iter()
            .all(|node| harness.semantics(*node).role == Role::Row)
    );

    // Every day is named in full: a reader moving through a grid of bare numbers has to hold the
    // month in their head.
    let first_of_month = days
        .iter()
        .find(|node| harness.attribute(**node, "data-date").as_deref() == Some("2026-07-01"))
        .copied()
        .expect("July starts somewhere in the grid");
    assert_eq!(
        harness.semantics(first_of_month).label.as_deref(),
        Some("Wednesday 1 July 2026"),
    );
    assert_eq!(harness.semantics(first_of_month).role, Role::GridCell);
}

#[test]
fn the_arrow_keys_walk_the_calendar_a_day_and_a_week_at_a_time() {
    let harness = Harness::open();
    let grid = calendar(&harness);
    assert_eq!(tab_stop(&harness).as_deref(), Some("2026-07-15"));

    harness.press(grid, NamedKey::ArrowRight);
    assert_eq!(tab_stop(&harness).as_deref(), Some("2026-07-16"));

    harness.press(grid, NamedKey::ArrowDown);
    assert_eq!(tab_stop(&harness).as_deref(), Some("2026-07-23"));

    harness.press(grid, NamedKey::ArrowUp);
    harness.press(grid, NamedKey::ArrowLeft);
    assert_eq!(tab_stop(&harness).as_deref(), Some("2026-07-15"));

    harness.press(grid, NamedKey::Home);
    assert_eq!(
        tab_stop(&harness).as_deref(),
        Some("2026-07-12"),
        "the week starts on Sunday in the default locale",
    );
    harness.press(grid, NamedKey::End);
    assert_eq!(tab_stop(&harness).as_deref(), Some("2026-07-18"));
}

#[test]
fn walking_off_the_edge_of_a_month_shows_the_next_one() {
    let harness = Harness::open();
    let grid = calendar(&harness);
    let heading = harness.find("zui-calendar__heading");
    assert_eq!(harness.window.dom.tree().text_content(heading), "July 2026");

    harness.press(grid, NamedKey::PageDown);
    assert_eq!(
        harness.window.dom.tree().text_content(heading),
        "August 2026"
    );
    assert_eq!(tab_stop(&harness).as_deref(), Some("2026-08-15"));

    // And a year at a time, with shift.
    harness
        .window
        .dispatcher()
        .with_modifiers(zgui::vocab::Modifiers::SHIFT)
        .key(grid, zgui::vocab::Key::Named(NamedKey::PageUp));
    harness.window.frame();
    assert_eq!(
        harness.window.dom.tree().text_content(heading),
        "August 2025"
    );
}

#[test]
fn a_calendar_holds_one_tab_stop_however_many_days_it_shows() {
    let harness = Harness::open();
    let grid = calendar(&harness);
    let sequential = all_with(&harness, "zui-calendar__day")
        .into_iter()
        .filter(|node| harness.attribute(*node, "tabindex").as_deref() == Some("0"))
        .count();
    assert_eq!(
        sequential, 1,
        "forty-two tab stops is a calendar nobody can tab past",
    );
    let _ = grid;
}

#[test]
fn pressing_a_day_chooses_it_and_pressing_it_again_clears_it() {
    let harness = Harness::open();
    let chosen: Rc<std::cell::RefCell<Vec<Option<Date>>>> = Rc::default();
    let record = Rc::clone(&chosen);
    harness.mount(move || {
        view! {
            Calendar(
                default_month = Date::new(2026, 7, 1).expect("a real date"),
                label = "Arrival",
                on_change = UnsyncCallback::new(move |date: Option<Date>| {
                    record.borrow_mut().push(date);
                })
            )
        }
    });

    let day = all_with(&harness, "zui-calendar__day")
        .into_iter()
        .find(|node| harness.attribute(*node, "data-date").as_deref() == Some("2026-07-09"))
        .expect("the ninth is in July's grid");

    harness.click(day);
    assert_eq!(
        harness.attribute(day, "data-selected").as_deref(),
        Some("true")
    );
    assert_eq!(harness.semantics(day).selected, Some(true));

    harness.click(day);
    assert_eq!(
        harness.attribute(day, "data-selected").as_deref(),
        Some("false")
    );
    assert_eq!(
        *chosen.borrow(),
        vec![Date::new(2026, 7, 9), None],
        "a calendar with no way back makes a mistake permanent",
    );
}

#[test]
fn a_calendar_opens_on_the_month_of_the_day_it_is_showing_however_that_day_is_held() {
    // The controlled form is the one in the documentation and the one an application with a form
    // behind it writes, and a calendar that opened on the epoch because the day it is showing as
    // chosen arrived through a signal rather than through `default_value` would be showing the
    // wrong month to everybody who did not also pass a month.
    for controlled in [false, true] {
        let harness = Harness::open();
        let day = Date::new(2026, 3, 9).expect("a real date");
        let held = harness.window.scope.with(|| RwSignal::new_local(Some(day)));
        harness.mount(move || {
            if controlled {
                AnyView::new(view! { Calendar(value = held, label = "Arrival") })
            } else {
                AnyView::new(view! { Calendar(default_value = day, label = "Arrival") })
            }
        });

        let heading = harness.find("zui-calendar__heading");
        assert_eq!(
            harness.window.dom.tree().text_content(heading),
            "March 2026",
            "a calendar holding a day through {} opened somewhere else",
            if controlled {
                "`value`"
            } else {
                "`default_value`"
            },
        );
        assert_eq!(tab_stop(&harness).as_deref(), Some("2026-03-09"));
    }
}

#[test]
fn a_calendar_with_nothing_chosen_opens_on_the_day_the_caller_calls_today() {
    let harness = Harness::open();
    harness.mount(|| {
        view! { Calendar(today = Date::new(2026, 11, 3).expect("a real date"), label = "When") }
    });
    assert_eq!(
        harness
            .window
            .dom
            .tree()
            .text_content(harness.find("zui-calendar__heading")),
        "November 2026",
    );
}

// ---- the chart ---------------------------------------------------------------------------------

#[test]
fn every_bar_is_its_own_shape_with_its_own_name_and_its_own_tab_stop() {
    let harness = Harness::open();
    harness.mount(|| {
        let series = vec![Series::new(
            "Units",
            vec![
                Datum::new("Jan", 120.0),
                Datum::new("Feb", 180.0),
                Datum::new("Mar", 90.0),
            ],
        )];
        view! { Chart(series = series, label = "Units sold", width = 300.0, height = 150.0) }
    });

    let marks: Vec<NodeId> = all_with(&harness, "zui-chart__mark")
        .into_iter()
        .filter(|node| harness.attribute(*node, "data-point").is_some())
        .collect();
    assert_eq!(marks.len(), 3, "one element per bar, not one picture");

    for (index, mark) in marks.iter().enumerate() {
        assert_eq!(
            harness.window.dom.tree().element_name(*mark).as_deref(),
            Some("vector"),
            "a mark is vector content, so it goes through the path renderer",
        );
        let path = harness
            .window
            .dom
            .tree()
            .property(*mark, PropKey::new("d"))
            .expect("every mark carries its own outline");
        assert!(
            format!("{path:?}").contains('M'),
            "mark {index} has no geometry",
        );
        assert_eq!(
            harness.attribute(*mark, "tabindex").as_deref(),
            Some("0"),
            "a Tab walk has to reach mark {index}",
        );
        assert_eq!(harness.semantics(*mark).role, Role::Image);
    }

    assert_eq!(
        harness.semantics(marks[1]).label.as_deref(),
        Some("Units, Feb, 180"),
        "a mark is read out as its series, its category and its value",
    );

    // Each mark is placed at its own geometry. Marks that all covered the plot would be stacked on
    // top of one another, and a pointer, a hit test and `:hover` all answer from a box — so only
    // whichever mark was built last would ever be reachable, however many elements there were.
    let boxes: Vec<(String, String, String)> = marks
        .iter()
        .map(|mark| {
            let read = |name: &str| {
                harness
                    .window
                    .dom
                    .tree()
                    .custom_property(*mark, CustomPropertyName::new(name))
                    .unwrap_or_else(|| panic!("every mark states its own `{name}`"))
            };
            (
                read("zui-chart-mark-x"),
                read("zui-chart-mark-width"),
                read("zui-chart-mark-height"),
            )
        })
        .collect();
    for pair in boxes.windows(2) {
        assert_ne!(
            pair[0].0, pair[1].0,
            "two marks in the same place: {boxes:?}"
        );
    }
    assert_ne!(
        boxes[0].2, boxes[2].2,
        "the tallest and the shortest bar are the same height",
    );
    let plot_width: f32 = 300.0;
    let bar_width: f32 = boxes[0]
        .1
        .trim_end_matches("px")
        .parse()
        .expect("a length in pixels");
    assert!(
        bar_width > 0.0 && bar_width < plot_width,
        "a bar is the whole plot wide, so it covers its neighbours",
    );
    assert!(
        zgui_ui::chart::ChartStyle::CSS.contains("--zui-chart-mark-x"),
        "the sheet ignores the box each mark states",
    );

    // The axes and the grid are one element, and it is not one a reader meets.
    let axes = all_with(&harness, "zui-chart__axes");
    assert_eq!(axes.len(), 1);
    assert!(harness.attribute(axes[0], "tabindex").is_none());
    assert!(
        harness
            .semantics(axes[0])
            .flags
            .contains(zgui::vocab::SemanticFlags::HIDDEN),
    );
}

#[test]
fn a_line_chart_draws_one_connecting_path_and_a_mark_at_every_point() {
    let harness = Harness::open();
    harness.mount(|| {
        let series = vec![Series::new(
            "Load",
            (0..5)
                .map(|step| Datum::new(format!("t{step}"), f64::from(step) * 3.0))
                .collect(),
        )];
        view! { Chart(series = series, kind = ChartKind::Line, label = "Load", legend = false) }
    });

    let marks = all_with(&harness, "zui-chart__mark");
    let points = marks
        .iter()
        .filter(|node| harness.attribute(**node, "data-point").is_some())
        .count();
    assert_eq!(points, 5);
    assert_eq!(
        marks.len(),
        points + 1,
        "the line itself is one more element, and only one",
    );
}

// ---- the date picker ---------------------------------------------------------------------------

#[test]
fn a_date_picker_opens_a_calendar_and_closes_when_a_day_is_chosen() {
    let harness = Harness::open();
    let taken: Rc<std::cell::RefCell<Vec<Option<Date>>>> = Rc::default();
    let record = Rc::clone(&taken);
    harness.mount(move || {
        view! {
            DatePicker(
                default_value = Date::new(2026, 7, 1).expect("a real date"),
                label = "Due date",
                on_change = UnsyncCallback::new(move |date: Option<Date>| {
                    record.borrow_mut().push(date);
                })
            )
        }
    });

    let trigger = harness.find("zui-date-picker__trigger");
    assert_eq!(
        harness.attribute(trigger, "aria-expanded").as_deref(),
        None,
        "the trigger states expansion through semantics rather than through an attribute",
    );
    assert_eq!(harness.semantics(trigger).expanded, Some(false));
    assert!(all_with(&harness, "zui-calendar__day").is_empty());

    harness.click(trigger);
    assert_eq!(harness.semantics(trigger).expanded, Some(true));
    let days = all_with(&harness, "zui-calendar__day");
    assert_eq!(days.len(), 42, "the calendar is on the surface");

    let ninth = days
        .into_iter()
        .find(|node| harness.attribute(*node, "data-date").as_deref() == Some("2026-07-09"))
        .expect("the ninth is in July's grid");
    harness.click(ninth);

    assert_eq!(*taken.borrow(), vec![Date::new(2026, 7, 9)]);
    assert_eq!(
        harness.semantics(trigger).expanded,
        Some(false),
        "choosing a day left the picker open",
    );
}

#[test]
fn a_date_picker_names_the_day_it_holds_and_the_placeholder_when_it_holds_none() {
    let harness = Harness::open();
    harness.mount(|| {
        view! { DatePicker(label = "Due date", placeholder = "Pick a date") }
    });
    let trigger = harness.find("zui-date-picker__trigger");
    assert_eq!(
        harness.window.dom.tree().text_content(trigger),
        "Pick a date",
    );
    assert_eq!(
        harness.attribute(trigger, "data-empty").as_deref(),
        Some("true")
    );
    assert_eq!(
        harness.semantics(trigger).label.as_deref(),
        Some("Due date")
    );
}

#[test]
fn a_date_picker_opens_on_the_month_it_is_showing_and_escape_closes_it_again() {
    let harness = Harness::open();
    let held = harness
        .window
        .scope
        .with(|| RwSignal::new_local(Date::new(2026, 3, 9)));
    harness.mount(move || {
        view! { DatePicker(value = held, label = "Due date") }
    });

    let trigger = harness.find("zui-date-picker__trigger");
    // Pressing a control focuses it, which is what the trap is going to restore to.
    zgui::view::ViewHost::focus(&*harness.window.host, trigger);
    harness.click(trigger);
    assert_eq!(
        harness
            .window
            .dom
            .tree()
            .text_content(harness.find("zui-calendar__heading")),
        "March 2026",
        "the surface opened on a month the picker is not showing",
    );

    // Escape belongs to the surface, not to the calendar: the grid deliberately claims neither
    // Escape nor Tab so that a calendar inside a popover can be left.
    harness.press(harness.find("zui-calendar__grid"), NamedKey::Escape);
    harness.window.advance(core::time::Duration::from_millis(1));
    assert_eq!(
        harness.semantics(trigger).expanded,
        Some(false),
        "escape did not reach the surface the calendar is on",
    );
    assert!(
        all_with(&harness, "zui-calendar__day").is_empty(),
        "the surface said it was closed and left its calendar in the tree",
    );
    assert_eq!(
        harness.window.host.focused().get_untracked(),
        Some(trigger),
        "closing gave the focus to nothing, so the reader is nowhere",
    );
}

// ---- the sheet the boxless row rests on ---------------------------------------------------------

#[test]
fn the_table_ships_the_boxless_row_the_engine_guarantees_keeps_matching() {
    // The engine's own probe for this lives beside the hit chain, where a `display: contents` row is
    // hovered through a real layout and a real cascade. This is the other half of that claim: that
    // the sheet this library actually ships is the shape the probe covers, so the two cannot drift
    // apart without one of them failing.
    let css = zgui_ui::table::TableStyle::CSS;
    assert!(
        css.contains(".zui-table__row { display: contents; }"),
        "the row generates a box, so its cells are not the grid's items",
    );
    assert!(
        css.contains(".zui-table__section { display: contents; }"),
        "the section generates a box, so a header and a body would be two grids",
    );
    assert!(
        css.contains(".zui-table__row:hover .zui-table__cell"),
        "the row highlight is written against something other than the boxless row",
    );

    // And the shipped classes are the ones a real table puts on its elements.
    let harness = Harness::open();
    let source = harness.window.scope.with(|| RwSignal::new_local(rows(2)));
    harness.mount(move || {
        view! {
            DataTable(
                rows = source,
                columns = columns(),
                row_id = |row: &Row| row.id.to_string(),
                label = "Rows"
            )
        }
    });
    assert_eq!(
        all_with(&harness, "zui-table__row").len(),
        3,
        "a header and two rows"
    );
    assert!(!all_with(&harness, "zui-table__section").is_empty());
    assert!(!all_with(&harness, "zui-table__cell").is_empty());
}

#[test]
fn scrolling_a_virtualised_table_does_not_sort_it_again() {
    // The claim virtualisation rests on: a body that re-reads its window on every scroll frame must
    // not re-derive the rows behind it. A derived signal is recomputed on every read, so this is
    // the difference between scrolling ten thousand rows and sorting them for every pixel.
    let comparisons = Rc::new(std::cell::Cell::new(0_usize));
    let counted = Rc::clone(&comparisons);

    let harness = Harness::open();
    let source = harness
        .window
        .scope
        .with(|| RwSignal::new_local(rows(2_000)));
    let grid = harness.window.scope.with(NodeRef::new);
    harness.mount(move || {
        let columns = vec![
            Column::new("name", "Name", |row: &Row| row.name.clone()).sortable_by(
                move |left: &Row, right: &Row| {
                    counted.set(counted.get() + 1);
                    left.name.cmp(&right.name)
                },
            ),
        ];
        view! {
            DataTable(
                rows = source,
                columns = columns,
                row_id = |row: &Row| row.id.to_string(),
                label = "Rows",
                virtualized = true,
                row_size = ROW,
                node_ref = grid
            )
        }
    });

    let table = harness.find("zui-table");
    harness.click(all_with(&harness, "zui-data-table__sort")[0]);
    let after_sorting = comparisons.get();
    assert!(after_sorting > 0, "sorting compared nothing at all");

    for step in 1..=50 {
        harness.window.dom.deliver(
            table,
            ObservedValue::ScrollPosition(scrolled(ROW * step as f32, 2_000)),
        );
        harness.window.frame();
    }

    assert_eq!(
        comparisons.get(),
        after_sorting,
        "fifty scroll frames re-sorted the table {} times over",
        (comparisons.get() - after_sorting) / after_sorting.max(1),
    );
    assert!(
        !all_with(&harness, "zui-table__row").is_empty(),
        "and the rows are still there, so the scroll really happened",
    );
}

#[test]
fn a_caller_handed_the_model_drives_the_table_from_outside_it() {
    // The escape hatch, exercised: everything the built-in toolbar does is a method on the model,
    // so a search box elsewhere on the page needs the model rather than a new prop.
    let held: Rc<std::cell::RefCell<Option<DataModel<Row>>>> = Rc::default();
    let keep = Rc::clone(&held);

    let harness = Harness::open();
    let source = harness.window.scope.with(|| RwSignal::new_local(rows(20)));
    harness.mount(move || {
        view! {
            DataTable(
                rows = source,
                columns = columns(),
                row_id = |row: &Row| row.id.to_string(),
                label = "Rows",
                on_model = UnsyncCallback::new(move |model: DataModel<Row>| {
                    *keep.borrow_mut() = Some(model);
                })
            )
        }
    });

    assert_eq!(first_column(&harness).len(), 20);
    let model = held.borrow().expect("the table handed its model over");

    model.set_filter("row-0003");
    harness.window.frame();
    assert_eq!(
        first_column(&harness),
        vec![String::from("row-0003")],
        "narrowing the model from outside did not narrow the tree",
    );

    model.press_header("size");
    model.set_filter("");
    harness.window.frame();
    assert_eq!(
        first_column(&harness).first().map(String::as_str),
        Some("row-0000"),
        "sorting the model from outside did not reorder the tree",
    );
}
