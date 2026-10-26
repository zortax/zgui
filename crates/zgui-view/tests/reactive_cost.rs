//! What one reactive part of a view holds: live bytes and live blocks, per kind.
//!
//! A counting allocator charges each allocation on the test's own thread. Each case builds
//! [`N`] parts of one kind, keeps them, and divides what is live by [`N`]. The stub document's
//! own nodes are counted too, so the cases that make nodes say how many.
//!
//! `cargo test --release -p zgui-view --test reactive_cost -- --ignored --nocapture` prints the
//! table.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::rc::Rc;

use zgui_interned::ElementName;
use zgui_reactive::prelude::*;
use zgui_reactive::{Memo, Mounted, RenderEffect, RwSignal, Selector, Sink, flush, install};
use zgui_view::stub::{StubDom, StubHost};
use zgui_view::view::Scoped;
use zgui_view::{Anchor, AnyView, BuildCxOwned, DocumentId, DomHandle, For, HostHandle, View};

/// Parts per case.
const N: usize = 1000;

thread_local! {
    /// Bytes this thread holds.
    static LIVE_BYTES: Cell<i64> = const { Cell::new(0) };
    /// Blocks this thread holds.
    static LIVE_BLOCKS: Cell<i64> = const { Cell::new(0) };
    /// Allocations this thread made.
    static ALLOCS: Cell<i64> = const { Cell::new(0) };
}

/// Counts what the current thread holds.
struct Counting;

// SAFETY: every call goes to `System` with the caller's layout.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = LIVE_BYTES.try_with(|b| b.set(b.get() + layout.size() as i64));
        let _ = LIVE_BLOCKS.try_with(|b| b.set(b.get() + 1));
        let _ = ALLOCS.try_with(|b| b.set(b.get() + 1));
        // SAFETY: the caller's contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = LIVE_BYTES.try_with(|b| b.set(b.get() - layout.size() as i64));
        let _ = LIVE_BLOCKS.try_with(|b| b.set(b.get() - 1));
        // SAFETY: the caller's contract.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = LIVE_BYTES.try_with(|b| b.set(b.get() + new_size as i64 - layout.size() as i64));
        // SAFETY: the caller's contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// What the current thread holds: bytes and blocks.
fn live() -> (i64, i64) {
    (LIVE_BYTES.with(Cell::get), LIVE_BLOCKS.with(Cell::get))
}

/// One document, one window scope and one root.
struct World {
    /// The document, for counting its nodes.
    backend: Rc<StubDom>,
    /// The document behind its handle.
    dom: DomHandle,
    /// The window's scope.
    window: Mounted,
    /// The build context.
    cx: BuildCxOwned,
    /// An element to mount into.
    root: zgui_view::NodeId,
}

impl World {
    /// Assembles one.
    fn new() -> Self {
        install().ok();
        std::mem::forget(zgui_reactive::enter_non_reactive_zone());
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let dom = DomHandle::from_rc(backend.clone());
        let host = HostHandle::new(StubHost::default());
        let window = Mounted::new();
        let cx = BuildCxOwned::new(dom.clone(), host, window.owner().clone(), DocumentId::FIRST);
        let root = dom.create_element(ElementName::new("root"));
        Self {
            backend,
            dom,
            window,
            cx,
            root,
        }
    }
}

/// What one part holds.
struct Cost {
    /// Live bytes.
    bytes: f64,
    /// Live blocks.
    blocks: f64,
    /// Stub nodes.
    nodes: f64,
}

/// What each part that `build` keeps holds, given what `setup` made for it.
fn per_part<P, T>(
    name: &str,
    mut setup: impl FnMut(&World, usize) -> P,
    mut build: impl FnMut(&World, &P, usize) -> T,
) -> Cost {
    let world = World::new();
    // Warm the thread's lazily made state (slot lists, queues) with one part.
    let warm_input = setup(&world, usize::MAX);
    let warm = build(&world, &warm_input, usize::MAX);
    let inputs: Vec<P> = (0..N).map(|i| setup(&world, i)).collect();
    let nodes = world.backend.node_count();
    let mut kept = Vec::with_capacity(N);
    let before = live();
    for (i, input) in inputs.iter().enumerate() {
        kept.push(build(&world, input, i));
    }
    let after = live();
    let nodes = (world.backend.node_count() - nodes) as f64 / N as f64;
    let bytes = (after.0 - before.0) as f64 / N as f64;
    let blocks = (after.1 - before.1) as f64 / N as f64;
    println!("{name:<44} {bytes:>8.1} B {blocks:>6.2} blocks {nodes:>4.1} nodes");
    drop(kept);
    drop(inputs);
    drop((warm, warm_input));
    let World { window, .. } = world;
    window.unmount();
    Cost {
        bytes,
        blocks,
        nodes,
    }
}

/// A signal in the window's scope.
fn signal(w: &World, i: usize) -> RwSignal<usize> {
    w.window.with(|| RwSignal::new(i))
}

/// Nothing.
fn none(_: &World, _: usize) {}

#[test]
#[ignore = "diagnostic table; run with --ignored --nocapture"]
fn report_what_each_reactive_part_holds() {
    per_part("stub text node", none, |w, (), _| {
        w.dom.create_text("12345")
    });
    per_part("stub marker node", none, |w, (), _| w.dom.create_marker());
    per_part("owner child", none, |w, (), _| w.window.owner().child());
    per_part("owner child + one cleanup", none, |w, (), _| {
        let o = w.window.owner().child();
        o.with(|| zgui_reactive::on_cleanup_local(|| {}));
        o
    });
    per_part("signal", none, |w, (), i| signal(w, i));
    per_part("sink over a signal", signal, |w, s, _| {
        let s = *s;
        w.window.with(|| Sink::new(move || _ = s.get()))
    });
    per_part("render effect over a signal", signal, |w, s, _| {
        let s = *s;
        w.window.with(|| RenderEffect::new(move |_| _ = s.get()))
    });
    per_part("memo over a signal (unread)", signal, |w, s, _| {
        let s = *s;
        w.window.with(|| Memo::new(move |_| s.get().to_string()))
    });
    per_part(
        "sink over a memo",
        |w, i| {
            let s = signal(w, i);
            w.window.with(|| Memo::new(move |_| s.get().to_string()))
        },
        |w, m, _| {
            let m = *m;
            w.window.with(|| Sink::new(move || _ = m.get()))
        },
    );
    {
        let w = World::new();
        let sel = w.window.with(|| {
            let s = RwSignal::new(Some(0_usize));
            Selector::new(move || s.get())
        });
        per_part(
            "selector key, read by a sink",
            |w, _| w.window.owner().child(),
            |_, o, i| {
                let sel = sel.clone();
                o.with(|| Sink::new(move || _ = sel.is_selected(&Some(i))))
            },
        );
        per_part(
            "3 selectors, same key and scope (row)",
            |w, _| w.window.owner().child(),
            |_, o, i| {
                let (a, b, c) = (sel.clone(), sel.clone(), sel.clone());
                o.with(|| {
                    [
                        Sink::new(move || _ = a.is_selected(&Some(i))),
                        Sink::new(move || _ = b.is_selected(&Some(i))),
                        Sink::new(move || _ = c.is_selected(&Some(i))),
                    ]
                })
            },
        );
        w.window.unmount();
    }
    per_part("static text (with its stub node)", none, |w, (), i| {
        let mut s = i.to_string().build(&mut w.cx.cx());
        s.mount(&w.dom, w.root, None);
        s
    });
    per_part(
        "closure text hole (with its stub node)",
        signal,
        |w, sig, _| {
            let sig = *sig;
            w.window.with(|| {
                let mut s = (move || sig.get().to_string()).build(&mut w.cx.cx());
                s.mount(&w.dom, w.root, None);
                s
            })
        },
    );
    per_part("scoped (component) with unit body", none, |w, (), _| {
        let mut s = Scoped::new(|| ()).build(&mut w.cx.cx());
        s.mount(&w.dom, w.root, None);
        s
    });
    {
        let w = World::new();
        let rows = w.window.with(|| RwSignal::new(Vec::<usize>::new()));
        let mut list = w.window.with(|| {
            For::new(move || rows.get(), |r: &usize| *r, |_| AnyView::new(())).build(&mut w.cx.cx())
        });
        list.mount(&w.dom, w.root, None);
        let all: Vec<usize> = (0..=N)
            .map(|i| if i == N { usize::MAX } else { i })
            .collect();
        rows.set(vec![usize::MAX]);
        flush();
        let before = live();
        rows.set(all);
        flush();
        let after = live();
        println!(
            "{:<44} {:>8.1} B {:>6.2} blocks (unit rows, collection included)",
            "keyed row",
            (after.0 - before.0) as f64 / N as f64,
            (after.1 - before.1) as f64 / N as f64
        );
        list.unmount(&w.dom);
        w.window.unmount();
    }
}

/// A text hole is a sink: its node, a scope, the shared content, the sink's node and its closure.
#[test]
fn a_text_hole_holds_its_text_node_and_four_blocks() {
    let node = per_part("stub text node", none, |w, (), _| {
        w.dom.create_text("12345")
    });
    let hole = per_part("closure text hole", signal, |w, sig, _| {
        let sig = *sig;
        w.window.with(|| {
            let mut s = (move || sig.get().to_string()).build(&mut w.cx.cx());
            s.mount(&w.dom, w.root, None);
            s
        })
    });
    assert_eq!(hole.nodes, 1.0, "no marker node");
    let blocks = hole.blocks - node.blocks;
    let bytes = hole.bytes - node.bytes;
    assert!(blocks <= 4.0, "{blocks} blocks");
    assert!(bytes <= 512.0, "{bytes} bytes");
}

/// Allocations this thread made so far.
fn allocs() -> i64 {
    ALLOCS.with(Cell::get)
}

/// Allocations per part over a whole lifecycle: make, flush, drop, flush. Run twice, so the
/// second round shows the cost once the executor's own storage is at its peak.
#[test]
#[ignore = "diagnostic table; run with --ignored --nocapture"]
fn report_lifecycle_allocations() {
    let world = World::new();
    let waker = std::sync::Arc::new(zgui_reactive::TestWaker::default());
    zgui_reactive::set_frame_waker(waker.clone());
    let signals: Vec<RwSignal<usize>> = (0..N).map(|i| signal(&world, i)).collect();
    flush();
    for round in 0..2 {
        let before = (allocs(), live().1);
        let effects: Vec<_> = signals
            .iter()
            .map(|&s| {
                world
                    .window
                    .with(|| RenderEffect::new(move |_| _ = s.get()))
            })
            .collect();
        flush();
        drop(effects);
        flush();
        let made = (allocs() - before.0) as f64 / N as f64;
        let held = (live().1 - before.1) as f64 / N as f64;
        println!(
            "render effect lifecycle, round {round}: {made:>6.2} allocs, {held:>5.2} blocks held"
        );
    }
    for round in 0..2 {
        let before = (allocs(), live().1);
        let polls = Rc::new(Cell::new(0_usize));
        let node = Mounted::new();
        node.with(|| {
            for _ in 0..N {
                let polls = polls.clone();
                zgui_reactive::spawn_local(std::future::poll_fn(move |_| {
                    polls.set(polls.get() + 1);
                    std::task::Poll::<()>::Pending
                }));
            }
        });
        flush();
        waker.take();
        node.unmount();
        let pings = waker.take();
        let first = polls.get();
        flush();
        let made = (allocs() - before.0) as f64 / N as f64;
        let held = (live().1 - before.1) as f64 / N as f64;
        println!(
            "spawn_local lifecycle, round {round}: {made:>6.2} allocs, {held:>5.2} blocks held, \
             {pings} pings in unmount, {} polls after unmount",
            polls.get() - first
        );
    }
    drop(signals);
    world.window.unmount();
}
