//! What the order sync allocates for lists that stay as they are.
//!
//! A counting allocator counts the allocations made on the test's own thread.

mod support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use support::{Element, Harness};

thread_local! {
    /// Allocations this thread made.
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

/// Counts the allocations of the current thread.
struct Counting;

// SAFETY: every call goes to `System` with the caller's layout.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: the caller's contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        // SAFETY: the caller's contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Returns how many allocations `run` made on this thread.
fn allocations(run: impl FnOnce()) -> u64 {
    let before = ALLOCATIONS.with(Cell::get);
    run();
    ALLOCATIONS.with(Cell::get) - before
}

#[test]
fn order_marks_on_lists_that_stay_as_they_are_allocate_nothing() {
    let rows = (0..40)
        .map(|_| Element::new("row").children(vec![Element::new("cell")]))
        .collect();
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("list").children(rows)]),
        "root { display: block; width: 200px }
         list { display: block }
         row { display: block; height: 10px }
         cell { display: block; height: 10px; background: rgb(0, 0, 255) }",
        200.0,
        400.0,
    );
    harness.paint_everything();
    harness.paint_everything();
    let rows: Vec<_> = harness
        .elements("row")
        .into_iter()
        .map(|row| {
            let key = harness.document().store().key_of(row);
            harness.store.boxes_of(key)[0]
        })
        .collect();

    sync(&mut harness, &rows);
    let one = sync(&mut harness, &rows[..1]);
    let all = sync(&mut harness, &rows);

    assert_eq!(
        all, one,
        "the marks on 40 rows allocate more than the mark on one"
    );
}

/// Marks the lists of `marked` and of their parents, and returns how many allocations the next
/// frame makes.
///
/// A write through `get_mut` marks them, as a patch of the box tree does, and changes neither.
fn sync(harness: &mut Harness, marked: &[zgui_dom::side::BoxKey]) -> u64 {
    for &row in marked {
        harness.store.get_mut(row);
    }
    harness.clear_damage();
    allocations(|| {
        harness.paint();
    })
}
