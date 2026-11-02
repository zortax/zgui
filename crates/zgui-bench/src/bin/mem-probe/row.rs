//! The document: a port of the comparison study's data explorer, one keyed row per record.
//!
//! The row is the study's row as it stands: thirteen nodes (the row, six cells and six text nodes),
//! nine bindings, two memos and three selector subscriptions, under the same sheet. The chrome
//! around the table is ported too, so that a run here and a run of the study build the same
//! document.
//!
//! The view holds each record in one of two ways: as a [`RowState`] with one signal per field, as
//! the study does, or as a key into a [`KeyedFields`] table, whose field signals exist only while a
//! row reads them.

use zgui::prelude::*;
use zgui::reactive::{KeyedFields, Lens, Selector};
#[allow(
    unused_imports,
    reason = "the tag names the component and the macro names its props type"
)]
use zgui_ui::prelude::{Input, InputProps};
use zgui_ui::virtualize::Virtualize;

/// Height of an ordinary row in CSS pixels.
pub(crate) const ROW_H: f32 = 22.0;

/// The seed the study builds its records from.
const SEED: u64 = 0x5EED_1234;

/// Gap left between adjacent record ids, as in the study.
const ID_STRIDE: u64 = 16;

/// Row background palette.
const TINTS: [[u8; 3]; 6] = [
    [0xf7, 0xf7, 0xfa],
    [0xef, 0xf3, 0xf8],
    [0xf5, 0xf0, 0xf7],
    [0xf0, 0xf6, 0xf2],
    [0xf8, 0xf4, 0xee],
    [0xf2, 0xf2, 0xf5],
];

/// Category labels.
const CATEGORIES: [&str; 6] = ["Alloy", "Ceramic", "Fiber", "Polymer", "Resin", "Timber"];

/// The stems record names are made from.
const NAME_STEMS: [&str; 12] = [
    "bracket", "flange", "gasket", "spindle", "washer", "bushing", "coupler", "grommet", "ferrule",
    "shim", "collet", "bearing",
];

/// The words notes are made from.
const NOTE_WORDS: [&str; 16] = [
    "tolerance",
    "verified",
    "batch",
    "inbound",
    "hold",
    "reworked",
    "supplier",
    "audit",
    "pending",
    "shipped",
    "quarantine",
    "sampled",
    "nominal",
    "revised",
    "expedite",
    "backorder",
];

/// One logical record, as the study's model holds it.
pub(crate) struct Record {
    /// The durable identity.
    pub(crate) id: u64,
    /// The name cell.
    pub(crate) name: String,
    /// An index into [`CATEGORIES`].
    pub(crate) category: u8,
    /// The price, in cents; the cell shows whole units.
    pub(crate) price_cents: i64,
    /// The quantity cell.
    pub(crate) qty: u32,
    /// The wrappable note cell.
    pub(crate) note: String,
    /// Whether the row is tall.
    pub(crate) tall: bool,
    /// An index into [`TINTS`].
    pub(crate) tint: u8,
}

/// SplitMix64, so the records are byte-identical to the study's.
struct Rng(u64);

impl Rng {
    /// The next value.
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value below `n`, as an index.
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next_u64() % n as u64).unwrap_or(0)
    }
}

/// The study's records for `rows` rows.
pub(crate) fn records(rows: usize) -> Vec<Record> {
    let mut rng = Rng(SEED);
    (0..rows)
        .map(|i| {
            let stem = NAME_STEMS[rng.below(NAME_STEMS.len())];
            let w1 = NOTE_WORDS[rng.below(NOTE_WORDS.len())];
            let w2 = NOTE_WORDS[rng.below(NOTE_WORDS.len())];
            let w3 = NOTE_WORDS[rng.below(NOTE_WORDS.len())];
            Record {
                id: i as u64 * ID_STRIDE,
                name: format!("{stem}-{i:05}"),
                category: u8::try_from(rng.below(CATEGORIES.len())).unwrap_or(0),
                price_cents: 100 + i64::try_from(rng.below(90_000)).unwrap_or(0),
                qty: u32::try_from(rng.below(500)).unwrap_or(0),
                note: format!("{w1} {w2} {w3}"),
                tall: false,
                tint: u8::try_from(i % TINTS.len()).unwrap_or(0),
            }
        })
        .collect()
}

/// One record as the view holds it: a durable key and one signal per field.
#[derive(Clone, Copy)]
pub(crate) struct RowState {
    /// The durable key.
    pub(crate) key: u64,
    /// The name cell.
    pub(crate) name: RwSignal<String>,
    /// The price, in cents.
    pub(crate) price_cents: RwSignal<i64>,
    /// The quantity.
    pub(crate) qty: RwSignal<u32>,
    /// The note.
    pub(crate) note: RwSignal<String>,
    /// The category label.
    pub(crate) category: &'static str,
    /// Whether the row is tall.
    pub(crate) tall: RwSignal<bool>,
    /// The row's tint.
    pub(crate) tint: RwSignal<u8>,
}

/// One entry of the ordered collection: what the view keys a row by, and how it builds one.
pub(crate) trait ProbeRow: Copy + Send + Sync + 'static {
    /// The durable key.
    fn key(&self) -> u64;

    /// The row's view.
    fn view(
        self,
        hovered: Selector<Option<u64>>,
        selected: Selector<Option<u64>>,
        editing: Selector<Option<u64>>,
    ) -> impl IntoView;

    /// Writes a new quantity, as a one-cell edit does.
    fn bump_qty(&self, by: u32);
}

impl ProbeRow for RowState {
    fn key(&self) -> u64 {
        self.key
    }

    fn view(
        self,
        hovered: Selector<Option<u64>>,
        selected: Selector<Option<u64>>,
        editing: Selector<Option<u64>>,
    ) -> impl IntoView {
        view! { TableRow(r = self, hovered = hovered, selected = selected, editing = editing) }
    }

    fn bump_qty(&self, by: u32) {
        self.qty.update(|qty| *qty = (*qty + by) % 1_000);
    }
}

impl RowState {
    /// Must be called with a reactive owner current, or the signals belong to nobody.
    pub(crate) fn new(r: &Record) -> Self {
        Self {
            key: r.id,
            name: RwSignal::new(r.name.clone()),
            price_cents: RwSignal::new(r.price_cents),
            qty: RwSignal::new(r.qty),
            note: RwSignal::new(r.note.clone()),
            category: CATEGORIES[r.category as usize],
            tall: RwSignal::new(r.tall),
            tint: RwSignal::new(r.tint),
        }
    }
}

/// One record as the keyed view holds it: the cells, as plain values.
pub(crate) struct Cells {
    /// The name cell.
    name: String,
    /// The price, in cents.
    price_cents: i64,
    /// The quantity.
    qty: u32,
    /// The note.
    note: String,
    /// The category label.
    category: &'static str,
    /// Whether the row is tall.
    tall: bool,
    /// The row's tint.
    tint: u8,
}

impl Cells {
    /// The cells of `r`.
    pub(crate) fn new(r: &Record) -> Self {
        Self {
            name: r.name.clone(),
            price_cents: r.price_cents,
            qty: r.qty,
            note: r.note.clone(),
            category: CATEGORIES[r.category as usize],
            tall: r.tall,
            tint: r.tint,
        }
    }
}

/// The name cell.
const NAME: Lens<Cells, String> = Lens::new(0, |c| &c.name, |c| &mut c.name);
/// The price cell.
const PRICE: Lens<Cells, i64> = Lens::new(1, |c| &c.price_cents, |c| &mut c.price_cents);
/// The quantity cell.
const QTY: Lens<Cells, u32> = Lens::new(2, |c| &c.qty, |c| &mut c.qty);
/// The note cell.
const NOTE: Lens<Cells, String> = Lens::new(3, |c| &c.note, |c| &mut c.note);
/// Whether the row is tall.
const TALL: Lens<Cells, bool> = Lens::new(4, |c| &c.tall, |c| &mut c.tall);
/// The row's tint.
const TINT: Lens<Cells, u8> = Lens::new(5, |c| &c.tint, |c| &mut c.tint);

/// One record as the keyed view refers to it: a key into the table.
#[derive(Clone, Copy)]
pub(crate) struct KeyedRow {
    /// The durable key.
    key: u64,
    /// The table that holds the cells.
    table: KeyedFields<u64, Cells>,
}

impl KeyedRow {
    /// One entry per record, over one table of their cells.
    ///
    /// Must be called with a reactive owner current.
    pub(crate) fn all(records: &[Record]) -> Vec<Self> {
        let table = KeyedFields::from_values(records.iter().map(|r| (r.id, Cells::new(r))));
        records.iter().map(|r| Self { key: r.id, table }).collect()
    }
}

impl ProbeRow for KeyedRow {
    fn key(&self) -> u64 {
        self.key
    }

    fn view(
        self,
        hovered: Selector<Option<u64>>,
        selected: Selector<Option<u64>>,
        editing: Selector<Option<u64>>,
    ) -> impl IntoView {
        view! { KeyedTableRow(r = self, hovered = hovered, selected = selected, editing = editing) }
    }

    fn bump_qty(&self, by: u32) {
        self.table
            .update(&self.key, QTY, |qty| *qty = (*qty + by) % 1_000);
    }
}

/// The tooltip overlay, when one is open.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TooltipState {
    /// What it says.
    pub(crate) text: String,
    /// Where it sits.
    pub(crate) top: f32,
}

/// Everything the probe writes and the view reads.
#[derive(Clone, Copy)]
pub(crate) struct Ctx<R: ProbeRow> {
    /// The ordered collection. Every entry is a live row.
    pub(crate) rows: RwSignal<Vec<R>>,
    /// The hovered record.
    pub(crate) hover: RwSignal<Option<u64>>,
    /// The selected record.
    pub(crate) selected: RwSignal<Option<u64>>,
    /// The record being edited.
    pub(crate) editor_key: RwSignal<Option<u64>>,
    /// The editor's text.
    pub(crate) editor_text: RwSignal<String, LocalStorage>,
    /// The filter text.
    pub(crate) filter: RwSignal<String>,
    /// The summary count.
    pub(crate) summary_count: RwSignal<usize>,
    /// The summary total, in cents.
    pub(crate) summary_total: RwSignal<i64>,
    /// The accent colour.
    pub(crate) accent: RwSignal<[u8; 3]>,
    /// The root font size.
    pub(crate) font_size: RwSignal<f32>,
    /// The note column's width.
    pub(crate) note_w: RwSignal<f32>,
    /// The badge animation phase.
    pub(crate) anim: RwSignal<f32>,
    /// The tooltip, when one is open.
    pub(crate) tooltip: RwSignal<Option<TooltipState>>,
    /// A record shown outside the viewport.
    pub(crate) a11y_row: RwSignal<Option<R>>,
    /// The scroll container the rows live in.
    pub(crate) port: NodeRef,
    /// The column whose children are the rows.
    pub(crate) rows_node: NodeRef,
    /// The inline editor's field.
    pub(crate) editor_field: NodeRef,
    /// The editor panel.
    pub(crate) editor_panel: NodeRef,
}

impl<R: ProbeRow> Ctx<R> {
    /// Must be called with a reactive owner current.
    pub(crate) fn new() -> Self {
        Self {
            rows: RwSignal::new(Vec::new()),
            hover: RwSignal::new(None),
            selected: RwSignal::new(None),
            editor_key: RwSignal::new(None),
            editor_text: RwSignal::new_local(String::new()),
            filter: RwSignal::new(String::new()),
            summary_count: RwSignal::new(0),
            summary_total: RwSignal::new(0),
            accent: RwSignal::new([0x2f, 0x6f, 0xed]),
            font_size: RwSignal::new(14.0),
            note_w: RwSignal::new(260.0),
            anim: RwSignal::new(0.0),
            tooltip: RwSignal::new(None),
            a11y_row: RwSignal::new(None),
            port: NodeRef::new(),
            rows_node: NodeRef::new(),
            editor_field: NodeRef::new(),
            editor_panel: NodeRef::new(),
        }
    }
}

/// The row's background as a CSS colour, from its palette index.
fn tint_css(tint: u8) -> String {
    let [r, g, b] = TINTS[(tint as usize) % TINTS.len()];
    format!("rgb({r}, {g}, {b})")
}

/// A length in CSS pixels.
fn px(v: f32) -> String {
    format!("{v}px")
}

/// The rows a table builds.
type Rows<R> = Signal<Vec<R>, LocalStorage>;

/// A length in CSS pixels that a view reads.
type Length = Signal<f32, LocalStorage>;

/// One row: six cells, each a reactive hole over one field signal.
#[component]
fn TableRow(
    r: RowState,
    hovered: Selector<Option<u64>>,
    selected: Selector<Option<u64>>,
    editing: Selector<Option<u64>>,
) -> impl IntoView {
    let key = r.key;
    let some = Some(key);
    let (h, s, e) = (hovered, selected, editing);
    let price = Memo::new(move |_| (r.price_cents.get() / 100).to_string());
    let qty = Memo::new(move |_| r.qty.get().to_string());
    view! {
        row(
            class = "row",
            class:hov = move || h.is_selected(&some),
            class:sel = move || s.is_selected(&some),
            class:edit = move || e.is_selected(&some),
            class:tall = move || r.tall.get(),
            style:background-color = move || Some(tint_css(r.tint.get())),
            attr:data-key = Some(key.to_string()),
            a11y:role = Role::Row
        ) {
            text(class = "c c-key", a11y:role = Role::Cell) {{key.to_string()}}
            text(class = "c c-name", a11y:role = Role::Cell) {{move || r.name.get()}}
            text(class = "c c-price", a11y:role = Role::Cell) {{move || price.get()}}
            text(class = "c c-qty", a11y:role = Role::Cell) {{move || qty.get()}}
            text(class = "c c-note", a11y:role = Role::Cell) {{move || r.note.get()}}
            text(class = "c c-cat", a11y:role = Role::Cell) {{r.category}}
        }
    }
}

/// One row of the keyed view: the same nodes, bindings and memos as [`TableRow`], each reading one
/// cell from the table.
#[component]
fn KeyedTableRow(
    r: KeyedRow,
    hovered: Selector<Option<u64>>,
    selected: Selector<Option<u64>>,
    editing: Selector<Option<u64>>,
) -> impl IntoView {
    let KeyedRow { key, table } = r;
    let some = Some(key);
    let (h, s, e) = (hovered, selected, editing);
    let price = Memo::new(move |_| (table.get(&key, PRICE).unwrap_or_default() / 100).to_string());
    let qty = Memo::new(move |_| table.get(&key, QTY).unwrap_or_default().to_string());
    let category = table
        .with_untracked(&key, |c| c.category)
        .unwrap_or_default();
    view! {
        row(
            class = "row",
            class:hov = move || h.is_selected(&some),
            class:sel = move || s.is_selected(&some),
            class:edit = move || e.is_selected(&some),
            class:tall = move || table.get(&key, TALL).unwrap_or_default(),
            style:background-color = move || table.get(&key, TINT).map(tint_css),
            attr:data-key = Some(key.to_string()),
            a11y:role = Role::Row
        ) {
            text(class = "c c-key", a11y:role = Role::Cell) {{key.to_string()}}
            text(class = "c c-name", a11y:role = Role::Cell) {{move || table.get(&key, NAME).unwrap_or_default()}}
            text(class = "c c-price", a11y:role = Role::Cell) {{move || price.get()}}
            text(class = "c c-qty", a11y:role = Role::Cell) {{move || qty.get()}}
            text(class = "c c-note", a11y:role = Role::Cell) {{move || table.get(&key, NOTE).unwrap_or_default()}}
            text(class = "c c-cat", a11y:role = Role::Cell) {{category}}
        }
    }
}

/// The application, with every row built when `unvirtualized`, and windowed by [`Virtualize`]
/// otherwise.
#[component]
#[allow(
    unused_braces,
    reason = "the macro passes a row's block child to the keyed list as an argument"
)]
pub(crate) fn Root<R: ProbeRow>(ctx: Ctx<R>, unvirtualized: bool) -> impl IntoView {
    let hovered = Selector::new(move || ctx.hover.get());
    let selected = Selector::new(move || ctx.selected.get());
    let editing = Selector::new(move || ctx.editor_key.get());

    let (source, lead, trail): (Rows<R>, Length, Length) = if unvirtualized {
        (
            Signal::derive_local(move || ctx.rows.get()),
            Signal::stored_local(0.0),
            Signal::stored_local(0.0),
        )
    } else {
        let count = Signal::derive_local(move || ctx.rows.with(Vec::len));
        let seen = Virtualize::new(ctx.port, count, ROW_H, 0);
        let window = Signal::derive_local(move || seen.window());
        (
            Signal::derive_local(move || {
                let w = window.get();
                ctx.rows.with(|rows| {
                    let end = w.end().min(rows.len());
                    let first = w.first.min(end);
                    rows[first..end].to_vec()
                })
            }),
            Signal::derive_local(move || window.get().lead),
            Signal::derive_local(move || window.get().trail),
        )
    };

    let (h2, s2, e2) = (hovered.clone(), selected.clone(), editing.clone());
    let (h3, s3, e3) = (hovered.clone(), selected.clone(), editing.clone());

    let count_label = Memo::new(move |_| format!("{} rows", ctx.summary_count.get()));
    let total_label = Memo::new(move |_| format!("total {}", ctx.summary_total.get() / 100));
    view! {
        column(
            class = "app",
            var:--accent = move || { let [r, g, b] = ctx.accent.get(); Some(format!("rgb({r}, {g}, {b})")) },
            var:--accent-soft = move || { let [r, g, b] = ctx.accent.get(); Some(format!("rgba({r}, {g}, {b}, 0.18)")) },
            var:--note-w = move || Some(px(ctx.note_w.get())),
            style:font-size = move || Some(px(ctx.font_size.get())),
            a11y:role = Role::Group,
            a11y:label = "Explorer"
        ) {
            row(class = "bar") {
                text(class = "bar__label") {"filter"}
                text(class = "bar__value") {{move || ctx.filter.get()}}
                spacer()
                box(
                    class = "badge",
                    style:opacity = move || Some(format!("{:.3}", 0.4 + 0.6 * ctx.anim.get())),
                    style:transform = move || Some(format!("translateX({:.2}px)", -20.0 * ctx.anim.get()))
                ) {"live"}
            }
            row(class = "head") {
                text(class = "c c-key") {"key"}
                text(class = "c c-name") {"name"}
                text(class = "c c-price") {"price"}
                text(class = "c c-qty") {"qty"}
                text(class = "c c-note") {"note"}
                text(class = "c c-cat") {"category"}
            }
            scroll(class = "port", node_ref = ctx.port, a11y:role = Role::Table, a11y:label = "Records") {
                column(
                    class = "rows",
                    node_ref = ctx.rows_node,
                    style:padding-top = move || Some(px(lead.get())),
                    style:padding-bottom = move || Some(px(trail.get()))
                ) {
                    for r in move || source.get(), key = |r: &R| r.key() {
                        {{r.view(h2.clone(), s2.clone(), e2.clone())}}
                    }
                }
            }
            row(class = "summary") {
                text {{move || count_label.get()}}
                text {{move || total_label.get()}}
            }
            row(
                class = "editor",
                node_ref = ctx.editor_panel,
                attr:data-editor-key = move || ctx.editor_key.get().map(|k| k.to_string())
            ) {
                text(class = "editor__label") {{move || match ctx.editor_key.get() {
                    Some(k) => format!("edit #{k}"),
                    None => "no editor".to_string(),
                }}}
                Input(node_ref = ctx.editor_field, value = ctx.editor_text, label = "Name")
            }
            if move || ctx.tooltip.get().is_some() {
                box(
                    class = "tooltip",
                    style:top = move || ctx.tooltip.get().map(|t| px(t.top)),
                    a11y:role = Role::Tooltip
                ) {
                    text {{move || ctx.tooltip.get().map(|t| t.text).unwrap_or_default()}}
                }
            }
            if move || ctx.a11y_row.get().is_some() {
                box(class = "offscreen", a11y:role = Role::Group, a11y:label = "Requested row") {
                    {{
                        let (h, s, e) = (h3.clone(), s3.clone(), e3.clone());
                        move || ctx.a11y_row.get().map(|r| {
                            r.view(h.clone(), s.clone(), e.clone()).into_view()
                        })
                    }}
                }
            }
        }
    }
}

/// The study's sheet.
pub(crate) const SHEET: &str = css!(
    ":root {
        background-color: #ffffff;
        color: #1b1f27;
        font-family: \"Noto Sans\";
        font-size: 14px;
    }
    .app { width: 1280px; height: 800px; }
    .bar { height: 20px; align-items: center; gap: 8px; padding: 0 8px; }
    .bar__label { color: #6b7689; }
    .badge { padding: 0 8px; border-radius: 6px; background-color: var(--accent); color: #ffffff; }
    .head { height: 20px; align-items: center; color: var(--accent); }
    .port { height: 704px; }
    .rows { }
    .row { height: 22px; align-items: center; flex-shrink: 0; }
    .row.tall { height: 46px; }
    .row.hov { background-image: linear-gradient(rgba(238, 242, 247, 0.75), rgba(238, 242, 247, 0.75)); }
    .row.sel { background-image: linear-gradient(var(--accent-soft), var(--accent-soft)); }
    .row.edit { outline: 1px solid var(--accent); outline-offset: -1px; }
    .c { padding: 0 6px; white-space: nowrap; overflow: hidden; }
    .c-key { width: 70px; }
    .c-name { width: 170px; }
    .c-price { width: 70px; color: var(--accent); }
    .c-qty { width: 60px; }
    .c-note { width: var(--note-w); white-space: normal; }
    .c-cat { width: 90px; }
    .summary { height: 20px; align-items: center; gap: 16px; padding: 0 8px; }
    .editor { height: 28px; align-items: center; gap: 8px; padding: 0 8px; }
    .editor__label { color: #6b7689; }
    .tooltip {
        position: absolute; left: 320px; padding: 2px 8px; border-radius: 4px;
        background-color: #1b1f27; color: #ffffff;
    }
    .offscreen { position: absolute; left: -10000px; top: 0; }"
);
