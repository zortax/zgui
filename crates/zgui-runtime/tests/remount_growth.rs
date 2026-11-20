//! What a window keeps of the rows it unmounted and of the text versions it replaced.
//!
//! A window that mounts the same rows again, shows a label that changes every frame, or moves a
//! colour back and forth must come back to the level it held before. The brush table, the records
//! of which slot each element is drawn through, the shaped paragraphs and the scopes of a keyed
//! list each kept something of every row that ever left and of every slot an element left. Each
//! grew by a little per row and per change, so no frame was slower and only the live heap showed
//! it.
//!
//! The heap is read through a counting allocator, which counts the whole process, so the target
//! holds these tests alone and they run one at a time.

#![allow(unsafe_code)]

mod support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use zgui_platform_headless::Harness;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_runtime::Runtime;
use zgui_runtime::budget::limits::IDLE_SHAPED_PARAGRAPHS;
use zgui_view::{BuildCx, ForProps, IntoView, View};

/// Bytes allocated and not yet freed, over the whole process.
static LIVE: AtomicI64 = AtomicI64::new(0);

/// The system allocator, with every live byte counted.
struct Counting;

// SAFETY: every call forwards to the system allocator with the arguments it was given, and only
// adds the change in live bytes to a counter.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged; the caller upholds `alloc`'s contract.
        let block = unsafe { System.alloc(layout) };
        if !block.is_null() {
            LIVE.fetch_add(layout.size() as i64, Ordering::Relaxed);
        }
        block
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged; the caller upholds `alloc_zeroed`'s contract.
        let block = unsafe { System.alloc_zeroed(layout) };
        if !block.is_null() {
            LIVE.fetch_add(layout.size() as i64, Ordering::Relaxed);
        }
        block
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged; the caller upholds `dealloc`'s contract.
        unsafe { System.dealloc(block, layout) };
        LIVE.fetch_sub(layout.size() as i64, Ordering::Relaxed);
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: forwarded unchanged; the caller upholds `realloc`'s contract.
        let moved = unsafe { System.realloc(block, layout, size) };
        if !moved.is_null() {
            LIVE.fetch_add(size as i64 - layout.size() as i64, Ordering::Relaxed);
        }
        moved
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The tests share the counter, so they run one at a time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Every cell colour is its own rule, so every cell is drawn through a brush of its own.
const CSS: &str = "
.root { display: block; width: 400px }
.row { display: flex }
.c { display: block; width: 60px }
.c0 { color: rgb(200, 0, 0) }
.c1 { color: rgb(0, 200, 0) }
.c2 { color: rgb(0, 0, 200) }
.c3 { color: rgb(90, 90, 90) }
";

/// How many rows the list mounts.
const ROWS: u32 = 120;

/// How many cells a row holds.
const CELLS: u32 = 4;

/// One frame at 60 Hz.
const TICK: Duration = Duration::from_micros(16_667);

/// What the window holds, as the tests compare it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Held {
    /// Shaped paragraphs.
    paragraphs: usize,
    /// Brush slots in use.
    slots: usize,
    /// Cascade results that resolve to a brush slot.
    keys: usize,
}

/// What the window holds now, read from the text engine's own tables.
fn held(harness: &mut Harness<Runtime>) -> Held {
    let window = &mut harness.app_mut().windows_mut()[0];
    let (slots, keys) = window.text_brushes_held();
    Held {
        paragraphs: window.shaped_held().paragraphs,
        slots,
        keys,
    }
}

/// Settles the window, then draws frames with nothing changed, so that what a window gives back on
/// a stride of frames is given back.
fn age(harness: &mut Harness<Runtime>) {
    harness.settle(64);
    for _ in 0..200 {
        harness.app().windows()[0].request_frame();
        harness.advance(TICK);
        harness.pump();
    }
    harness.settle(64);
    drain(harness);
}

/// Empties the headless surface's accessibility log, as a platform's listener does.
///
/// The log is the test platform's own record of what it was told, and it holds every update up to
/// its bound. Left in place, it would count as heap the window keeps.
fn drain(harness: &Harness<Runtime>) {
    for surface in harness.platform().offscreens() {
        drop(surface.take_a11y_log());
    }
}

/// The live heap now, in bytes.
fn live() -> i64 {
    LIVE.load(Ordering::Relaxed)
}

/// A window with a keyed list of `rows`, each row a cell per colour holding static text.
fn list(rows: RwSignal<Vec<u32>>) -> Harness<Runtime> {
    support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    ForProps::builder()
                        .each(move || rows.get())
                        .key(|row: &u32| *row)
                        .children(move |row: u32| {
                            // An inline style per row keeps the rows from sharing one cascade
                            // result, so every cell is drawn through a brush slot of its own.
                            let mut built = zgui_elements::row().class("row").style_text(Some(
                                format!("background-color: rgb({}, 250, 250)", row % 250),
                            ));
                            for cell in 0..CELLS {
                                built = built.child(
                                    zgui_elements::text()
                                        .class(format!("c c{cell}"))
                                        .child(format!("{row}-{cell}")),
                                );
                            }
                            built
                        })
                        .build()
                        .render(),
                )
                .into_view()
                .build(cx),
        )
    })
}

#[test]
fn rows_mounted_again_leave_the_window_where_it_was() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let all: Vec<u32> = (0..ROWS).collect();
    let rows = RwSignal::new(all.clone());
    let mut harness = list(rows);
    age(&mut harness);

    let cycle = |harness: &mut Harness<Runtime>| {
        rows.set(Vec::new());
        age(harness);
        let empty = live();
        rows.set(all.clone());
        age(harness);
        empty
    };
    // The first cycles grow tables to the size a cycle needs.
    let mut empty = 0;
    for _ in 0..3 {
        empty = cycle(&mut harness);
    }
    let level = held(&mut harness);
    let bytes = live();
    let rows_bytes = bytes - empty;
    assert!(
        level.slots >= (ROWS * CELLS) as usize,
        "every cell claims a slot of its own: {level:?}"
    );

    // Caches that sweep when they double hold the styles of rows that left for a while, so the
    // level rises and falls from cycle to cycle. It has to stay below one more copy of the rows,
    // and on average come back.
    const CYCLES: i64 = 48;
    let mut peak = bytes;
    for turn in 0..CYCLES {
        cycle(&mut harness);
        assert_eq!(
            held(&mut harness),
            level,
            "cycle {turn}: the brush table or the shaped paragraphs kept something of the rows \
             that left"
        );
        peak = peak.max(live());
    }
    assert!(
        peak - bytes < rows_bytes,
        "the window held {} bytes more than after the first cycles, against {rows_bytes} for the \
         mounted rows",
        peak - bytes
    );
    let per_cycle = (live() - bytes) / CYCLES;
    assert!(
        per_cycle * 50 < rows_bytes,
        "each cycle kept {per_cycle} bytes on average, against {rows_bytes} for the mounted rows: \
         more than 2 % of the rows stays behind every time they are mounted again"
    );
}

#[test]
fn a_label_that_keeps_changing_holds_a_bounded_number_of_old_versions() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let value = RwSignal::new(0_u32);
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().child("still"))
                .child(zgui_elements::text().child(move || format!("value {}", value.get())))
                .into_view()
                .build(cx),
        )
    });
    harness.settle(16);
    let edit = |harness: &mut Harness<Runtime>, at: u32| {
        value.set(at);
        harness.settle(4);
        drain(harness);
    };

    // Past the allowance for old versions, every new version lets the coldest one go.
    let warm = IDLE_SHAPED_PARAGRAPHS as u32 + 64;
    for at in 1..=warm {
        edit(&mut harness, at);
    }
    let paragraphs = held(&mut harness).paragraphs;
    assert!(
        paragraphs <= 2 + IDLE_SHAPED_PARAGRAPHS,
        "{paragraphs} shaped paragraphs for two current ones after {warm} versions of a label"
    );

    const EDITS: u32 = 256;
    let bytes = live();
    for at in warm + 1..=warm + EDITS {
        edit(&mut harness, at);
    }
    assert_eq!(held(&mut harness).paragraphs, paragraphs);
    let per_edit = (live() - bytes) / i64::from(EDITS);
    assert!(
        per_edit < 64,
        "each change of the label kept {per_edit} bytes after the old versions reached their bound"
    );
}

#[test]
fn a_colour_toggled_back_and_forth_reuses_the_slot_it_left() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Equal siblings share one cascade result and so one slot. The toggled one leaves it for a
    // slot of its own and comes back, and the slot it leaves behind each time has to be released.
    let hot = RwSignal::new(false);
    let mut harness = support::app_with_text(
        ".root { display: block } text { display: block } .hot { color: rgb(220, 0, 0) }",
        move |cx: &mut BuildCx<'_>| {
            let mut root = zgui_elements::column().class("root");
            for index in 0..4 {
                root = root.child(zgui_elements::text().child(format!("label {index}")));
            }
            Box::new(
                root.child(
                    zgui_elements::text()
                        .class_toggle(zgui_view::ClassName::new("hot"), move || hot.get())
                        .child("toggled"),
                )
                .into_view()
                .build(cx),
            )
        },
    );
    harness.settle(16);
    let toggle = |harness: &mut Harness<Runtime>, on: bool| {
        hot.set(on);
        harness.settle(8);
    };
    toggle(&mut harness, true);
    toggle(&mut harness, false);
    let level = held(&mut harness).slots;
    for _ in 0..16 {
        toggle(&mut harness, true);
        toggle(&mut harness, false);
    }
    assert_eq!(
        held(&mut harness).slots,
        level,
        "every toggle left a slot behind that nothing is drawn through"
    );
}

#[test]
fn a_toggle_that_reshapes_every_label_keeps_the_other_state_shaped() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // More labels than the floor of the allowance, so that only an allowance that follows the
    // current paragraphs keeps both states.
    const LABELS: usize = IDLE_SHAPED_PARAGRAPHS * 3 / 2;
    let big = RwSignal::new(false);
    let mut harness = support::app_with_text(
        ".root { display: block; width: 400px } text { display: block } .big { font-size: 20px }",
        move |cx: &mut BuildCx<'_>| {
            let mut root = zgui_elements::column()
                .class("root")
                .class_toggle(zgui_view::ClassName::new("big"), move || big.get());
            for index in 0..LABELS {
                root = root.child(zgui_elements::text().child(format!("label {index}")));
            }
            Box::new(root.into_view().build(cx))
        },
    );
    harness.settle(16);
    let toggle = |harness: &mut Harness<Runtime>, on: bool| {
        big.set(on);
        harness.settle(8);
    };
    toggle(&mut harness, true);
    toggle(&mut harness, false);
    let both = held(&mut harness).paragraphs;
    assert!(
        both >= 2 * LABELS,
        "{both} shaped paragraphs for {LABELS} labels in two sizes: one size is shaped again on \
         every toggle"
    );
    for _ in 0..2 {
        toggle(&mut harness, true);
        toggle(&mut harness, false);
    }
    assert_eq!(held(&mut harness).paragraphs, both);
}
