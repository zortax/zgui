//! The data surfaces: tables, charts, dates and a list with a hundred thousand rows.

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui::virtualize::Virtualize;

use crate::shell::PanelProps;

/// One line of an invoice.
#[derive(Clone)]
pub(crate) struct Line {
    /// What identifies it.
    id: u32,
    /// What was bought.
    item: String,
    /// What it cost, in pence.
    pence: i64,
}

/// Tables, charts, a calendar and a virtualised list.
#[component]
pub(crate) fn Data() -> impl IntoView {
    view! {
        Panel(title = "Table", note = "laid out by the column track list, not by a grid of boxes") {
            Table(columns = "1fr 120px", label = "Invoices") {
                TableCaption {"Last three months"}
                TableHeader {
                    TableRow {
                        TableHead(index = 0_usize) {"Invoice"}
                        TableHead(index = 1_usize, align = CellAlign::End) {"Amount"}
                    }
                }
                TableBody {
                    TableRow(index = 0_usize) {
                        TableCell(index = 0_usize, header = true) {"INV-001"}
                        TableCell(index = 1_usize, align = CellAlign::End) {"£250.00"}
                    }
                    TableRow(index = 1_usize) {
                        TableCell(index = 0_usize, header = true) {"INV-002"}
                        TableCell(index = 1_usize, align = CellAlign::End) {"£125.00"}
                    }
                    TableRow(index = 2_usize) {
                        TableCell(index = 0_usize, header = true) {"INV-003"}
                        TableCell(index = 1_usize, align = CellAlign::End) {"£42.00"}
                    }
                }
                TableFooter {
                    TableRow(index = 3_usize) {
                        TableCell(index = 0_usize) {"Total"}
                        TableCell(index = 1_usize, align = CellAlign::End) {"£417.00"}
                    }
                }
            }
        }

        Panel(title = "Data table", note = "sorted, searched, selected and paged", wide = true) {
            Lines()
        }

        Panel(title = "Chart", note = "the same series drawn three ways") {
            Charts()
        }

        Panel(title = "Calendar and date picker", note = "a month, and a month behind a button") {
            Dates()
        }

        Panel(title = "Virtualised list", note = "a hundred thousand rows, of which a couple of dozen exist") {
            Ledger()
        }
    }
}

/// Every line of the invoice, sortable and searchable.
#[component]
fn Lines() -> impl IntoView {
    let lines = RwSignal::new_local(vec![
        Line {
            id: 1,
            item: "Paper".into(),
            pence: 250,
        },
        Line {
            id: 2,
            item: "Ink".into(),
            pence: 1250,
        },
        Line {
            id: 3,
            item: "Binding".into(),
            pence: 400,
        },
        Line {
            id: 4,
            item: "Postage".into(),
            pence: 190,
        },
        Line {
            id: 5,
            item: "Envelopes".into(),
            pence: 320,
        },
    ]);
    let columns = vec![
        Column::new("item", "Item", |line: &Line| line.item.clone())
            .sortable_by(|a: &Line, b: &Line| a.item.cmp(&b.item)),
        Column::new("cost", "Cost", |line: &Line| format!("{}p", line.pence))
            .aligned(CellAlign::End)
            .sized("120px")
            .sortable_by(|a: &Line, b: &Line| a.pence.cmp(&b.pence)),
    ];

    view! {
        DataTable(
            rows = lines,
            columns = columns,
            row_id = |line: &Line| line.id.to_string(),
            label = "Invoice lines",
            selectable = true,
            filterable = true,
            page_size = 3_usize
        )
    }
}

/// The same numbers, as bars, as a line and as points.
#[component]
fn Charts() -> impl IntoView {
    let months = || {
        vec![Series::new(
            "Units",
            vec![
                Datum::new("Jan", 120.0),
                Datum::new("Feb", 180.0),
                Datum::new("Mar", 90.0),
                Datum::new("Apr", 160.0),
                Datum::new("May", 210.0),
            ],
        )]
    };

    view! {
        column(class = "stack") {
            ChartContainer {
                Chart(series = months(), label = "Units sold, as bars", kind = ChartKind::Bar)
                ChartLegendContent(
                    align = LegendAlign::Top,
                    entries = vec![
                        LegendEntry::new("Units", "var(--zui-color-chart-2)"),
                        LegendEntry::new("Returns", "var(--zui-color-chart-4)"),
                    ]
                )
            }
            Chart(series = months(), label = "Units sold, as a line", kind = ChartKind::Line)
            Chart(series = months(), label = "Units sold, as an area", kind = ChartKind::Area)
            ChartTooltipContent(
                label = "March",
                entries = vec![
                    ChartEntry::new("Units", 90.0, "var(--zui-color-chart-2)"),
                    ChartEntry::new("Returns", 12.0, "var(--zui-color-chart-4)"),
                ],
                indicator = ChartIndicator::Line
            )
            ChartTooltipContent(
                label = "April",
                entries = vec![
                    ChartEntry::new("Units", 104.0, "var(--zui-color-chart-2)"),
                    ChartEntry::new("Returns", 9.0, "var(--zui-color-chart-4)"),
                ],
                indicator = ChartIndicator::Dashed
            )
        }
    }
}

/// A month on the page, and a month behind a button.
#[component]
fn Dates() -> impl IntoView {
    let arrival = RwSignal::new_local(Date::new(2026, 7, 24));
    let due = RwSignal::new_local(None::<Date>);
    let stay = RwSignal::new_local(None::<DateRange>);

    view! {
        column(class = "stack") {
            Calendar(value = arrival, label = "Arrival date")
            Calendar(
                mode = CalendarMode::Range,
                range = stay,
                months = 2_usize,
                label = "Stay"
            )
            DatePicker(value = due, label = "Due date", placeholder = "Pick a date")
        }
    }
}

/// A hundred thousand rows, of which a couple of dozen exist.
#[component]
fn Ledger() -> impl IntoView {
    let port = NodeRef::new();
    let rows = RwSignal::new_local(100_000_usize);
    let seen = Virtualize::new(port, rows.into(), 24.0, 4);

    view! {
        scroll(node_ref = port, class = "tall frame") {
            column(
                style:padding-top = move || Some(format!("{}px", seen.window().lead)),
                style:padding-bottom = move || Some(format!("{}px", seen.window().trail))
            ) {
                for index in move || seen.window().indices(), key = |index: &usize| *index {
                    text {{move || format!("row {index}")}}
                }
            }
        }
    }
}
