//! Tables, charts, dates and the list with a hundred thousand rows.

use zgui::vocab::NamedKey;

use crate::script::find;
use crate::stage::Stage;

/// Drives the data surfaces.
pub(crate) fn run(stage: &mut Stage<'_>) {
    table(stage);
    data_table(stage);
    charts(stage);
    dates(stage);
    virtualised(stage);
}

/// Whether `text` is on the screen.
fn drawn(stage: &Stage<'_>, text: &str) -> bool {
    stage.shown(text)
}

/// The plain table.
fn table(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Table") else {
        stage.report.note("Table", "the panel is not laid out");
        return;
    };
    let cells = ["INV-001", "£250.00", "INV-003", "Total", "£417.00"]
        .iter()
        .filter(|text| find::at_in(&census, panel, text).is_some())
        .count();
    stage.report.check(
        "Table",
        "the header, body and footer rows are all laid out",
        cells == 5,
        &format!("{cells} of 5 cells"),
    );

    // The column track list is the point of this table: the amounts are in a 120px column and so
    // have to line up with each other rather than with the text beside them.
    let amounts: Vec<f32> = ["£250.00", "£125.00", "£42.00"]
        .iter()
        .filter_map(|text| census.control(text))
        .filter_map(|node| node.rect)
        .map(|rect| rect.origin.x.0 + rect.size.width.0)
        .collect();
    let aligned = amounts.len() == 3 && amounts.iter().all(|edge| (edge - amounts[0]).abs() < 1.5);
    stage.report.check(
        "Table",
        "the amounts share a column and are aligned to its end",
        aligned,
        &format!("their right edges are at {amounts:?}"),
    );
    stage.shot("data-table");
}

/// The data table, which is sorted, searched, selected and paged.
fn data_table(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Data table") else {
        stage.report.note("DataTable", "the panel is not laid out");
        return;
    };
    // One entry per cell, not one per box saying the cell's word. A cell is a box holding a box
    // holding the text, and every one of them answers to the same words — so counting boxes counts
    // each row twice, and a table that pages three rows at a time is reported as showing six.
    let rows = |stage: &Stage<'_>| -> Vec<String> {
        let census = stage.census();
        let matched: Vec<_> = census
            .inside(panel)
            .into_iter()
            .filter(|node| {
                ["Paper", "Ink", "Binding", "Postage", "Envelopes"].contains(&node.text.as_str())
                    && node.area() > 0.0
            })
            .collect();
        matched
            .iter()
            .filter(|node| {
                !matched.iter().any(|other| {
                    other.id != node.id && stage.handles().host.contains(node.id, other.id)
                })
            })
            .map(|node| node.text.clone())
            .collect()
    };
    let first = rows(stage);
    stage.report.check(
        "DataTable",
        "it pages rather than showing everything",
        first.len() == 3,
        &format!("the first page holds {first:?}"),
    );
    stage.shot("data-datatable-page-one");

    // Sorting by the item column has to reorder the rows, not just mark the header.
    if let Some(header) = find::at_in(&census, panel, "Item") {
        stage.click(header);
        stage.settle(6);
        let sorted = rows(stage);
        stage.report.check(
            "DataTable",
            "clicking a column heading sorts the rows",
            sorted != first && !sorted.is_empty(),
            &format!("{first:?} became {sorted:?}"),
        );
        stage.shot("data-datatable-sorted");
    }

    // The filter.
    let census = stage.census();
    let filter = census
        .inside(panel)
        .into_iter()
        .filter(|node| node.text.is_empty() || node.text.to_lowercase().contains("search"))
        .filter(|node| {
            node.rect.is_some_and(|rect| {
                rect.size.width.0 > 100.0 && rect.size.height.0 > 20.0 && rect.size.height.0 < 60.0
            })
        })
        .min_by(|left, right| {
            left.rect
                .map_or(0.0, |rect| rect.origin.y.0)
                .total_cmp(&right.rect.map_or(0.0, |rect| rect.origin.y.0))
        })
        .and_then(|node| node.centre());
    if let Some(filter) = filter {
        stage.click(filter);
        stage.type_text("ink");
        stage.settle(6);
        let found = rows(stage);
        stage.report.check(
            "DataTable",
            "typing in the filter narrows the rows",
            found == vec!["Ink".to_owned()],
            &format!("filtering for ink left {found:?}"),
        );
        stage.shot("data-datatable-filtered");
        for _ in 0..4 {
            stage.key(NamedKey::Backspace);
        }
        stage.settle(6);
    }
}

/// The three charts.
fn charts(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Chart") else {
        stage.report.note("Chart", "the panel is not laid out");
        return;
    };
    // A chart of five points has to produce marks, and the marks have to differ in size the way
    // the numbers do — otherwise it is a chart of nothing, drawn to the right shape.
    let marks: Vec<f32> = census
        .inside(panel)
        .into_iter()
        .filter(|node| node.text.is_empty() && node.area() > 0.0)
        .filter_map(|node| node.rect)
        .map(|rect| rect.size.height.0)
        .collect();
    stage.report.check(
        "Chart",
        "the charts produce marks",
        marks.len() >= 5,
        &format!("{} textless boxes in the panel", marks.len()),
    );
    let spread = marks.iter().copied().fold(f32::MIN, f32::max)
        - marks.iter().copied().fold(f32::MAX, f32::min);
    stage.report.check(
        "Chart",
        "the marks differ in size the way the numbers do",
        spread > 4.0,
        &format!("the tallest and shortest mark differ by {spread:.1} device pixels"),
    );
    stage.shot("data-charts");
}

/// The calendar and the date picker.
fn dates(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Calendar and date picker") else {
        stage.report.note("Calendar", "the panel is not laid out");
        return;
    };
    let days = census
        .inside(panel)
        .into_iter()
        .filter(|node| node.area() > 0.0 && node.text.parse::<u32>().is_ok_and(|day| day <= 31))
        .count();
    stage.report.check(
        "Calendar",
        "a month of days is laid out",
        days >= 28,
        &format!("{days} day cells"),
    );
    stage.shot("data-calendar");

    // Choosing a day: the picker starts empty and has to say what was chosen once it is.
    let Some(picker) = find::at_in(&census, panel, "Pick a date") else {
        stage.report.note("DatePicker", "no picker");
        return;
    };
    stage.click(picker);
    stage.settle(8);
    let opened = stage.census();
    let cell = opened
        .nodes
        .iter()
        .filter(|node| node.text == "17" && node.area() > 0.0)
        .max_by(|left, right| {
            left.rect
                .map_or(0.0, |rect| rect.origin.y.0)
                .total_cmp(&right.rect.map_or(0.0, |rect| rect.origin.y.0))
        })
        .and_then(|node| node.centre());
    stage.report.check(
        "DatePicker",
        "the button opens a month",
        cell.is_some(),
        "a day cell is laid out under the button",
    );
    stage.shot("data-datepicker-open");
    if let Some(cell) = cell {
        stage.click(cell);
        stage.settle(8);
        stage.report.check(
            "DatePicker",
            "choosing a day closes the month and puts the date on the button",
            !drawn(stage, "Pick a date"),
            &format!(
                "the placeholder is {}",
                if drawn(stage, "Pick a date") {
                    "still shown"
                } else {
                    "gone"
                }
            ),
        );
        stage.shot("data-datepicker-chosen");
    }

    // The calendar is walked with the arrows, which is what makes it usable without a pointer.
    // Where the panel is has to be asked again: a month has been opened and a day chosen since it
    // was measured, and a day found in a stale rectangle is a day in some other panel.
    let census = stage.census();
    let panel = find::panel(&census, "Calendar and date picker").unwrap_or(panel);
    if let Some(day) = census
        .inside(panel)
        .into_iter()
        .find(|node| node.text == "15" && node.area() > 0.0)
        .and_then(|node| node.centre())
    {
        stage.click(day);
        let before = stage.focused_text();
        // Which of the two calendars on the page the click landed in, and whether the cell it
        // focused is a control at all: a day that cannot be focused and a day whose arrows do
        // nothing report the same way otherwise.
        let cell = stage.focused();
        stage.report.note(
            "Calendar",
            &format!(
                "the click focused {cell:?} saying {before:?}, and the panel offers {} focusable \
                 nodes",
                stage
                    .census()
                    .inside(panel)
                    .into_iter()
                    .filter(|node| stage.handles().host.focusables(node.id).len() == 1)
                    .count()
            ),
        );
        stage.key(NamedKey::ArrowRight);
        let after = stage.focused_text();
        stage.report.check(
            "Calendar",
            "the arrows walk the days",
            !after.is_empty() && after != before,
            &format!("focus went from {before:?} to {after:?}"),
        );
    }
}

/// The virtualised list.
fn virtualised(stage: &mut Stage<'_>) {
    let Some((census, panel)) = find::open_panel(stage, "Virtualised list") else {
        stage.report.note("Virtualize", "the panel is not laid out");
        return;
    };
    // One row each, not the box around them all. Everything a list holds is also text of the list
    // itself, so the box that holds every row says "row 0row 1row 2…" and begins with "row " like
    // each of its children — and taken as the first row, it makes a list correctly starting at the
    // top look like one starting somewhere else.
    let rows = |stage: &Stage<'_>| -> Vec<String> {
        stage
            .census()
            .inside(panel)
            .into_iter()
            .filter(|node| node.text.starts_with("row ") && node.text.matches("row ").count() == 1)
            .filter(|node| node.area() > 0.0)
            .map(|node| node.text.clone())
            .collect()
    };
    let before = rows(stage);
    stage.report.check(
        "Virtualize",
        "only a window of the hundred thousand rows exists",
        (1..200).contains(&before.len()),
        &format!("{} rows are laid out", before.len()),
    );
    stage.report.check(
        "Virtualize",
        "the window starts at the top",
        before.first().map(String::as_str) == Some("row 0"),
        &format!("the first row is {:?}", before.first()),
    );
    stage.shot("data-virtual-top");

    let Some((node, _)) = census
        .inside(panel)
        .into_iter()
        .map(|node| (node.id, stage.handles().host.scroll_position(node.id)))
        .find(|(_, position)| position.content_size.height.0 > position.scrollport.height.0 + 1.0)
    else {
        stage.report.note("Virtualize", "nothing inside it scrolls");
        return;
    };

    stage.move_to(zgui::geom::Point::new(
        zgui::geom::DevicePx(panel.origin.x.0 + panel.size.width.0 / 2.0),
        zgui::geom::DevicePx(panel.origin.y.0 + panel.size.height.0 * 0.7),
    ));
    for _ in 0..12 {
        stage.wheel((0.0, 10.0));
    }
    let after = rows(stage);
    let offset = stage.handles().host.scroll_position(node);
    stage.report.check(
        "Virtualize",
        "scrolling brings rows from further down the list",
        after.first().map(String::as_str) != Some("row 0") && !after.is_empty(),
        &format!(
            "the first row is now {:?} at offset {:.0}",
            after.first(),
            offset.offset.y.0
        ),
    );
    stage.report.check(
        "Virtualize",
        "the window stays small however far it is scrolled",
        (1..200).contains(&after.len()),
        &format!("{} rows exist after scrolling", after.len()),
    );
    stage.shot("data-virtual-scrolled");
}
