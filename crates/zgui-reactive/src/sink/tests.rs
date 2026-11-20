use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use reactive_graph::computed::Memo;
use reactive_graph::effect::RenderEffect;
use reactive_graph::owner::Owner;
use reactive_graph::signal::RwSignal;
use reactive_graph::traits::{Get, GetUntracked, Set};

use super::Sink;
use crate::executor::{TestWaker, flush, install, set_frame_waker};
use crate::own::{Mounted, Scope};

/// Installs a runtime and a counting waker on this test's thread, under a root owner.
fn runtime() -> (Mounted, Arc<TestWaker>) {
    install().expect("no other executor is installed");
    let waker = Arc::new(TestWaker::default());
    set_frame_waker(waker.clone());
    (Mounted::new(), waker)
}

/// A sink that counts its runs and records the value `read` returns.
fn counting(read: impl Fn() -> i32 + 'static) -> (Sink, Rc<Cell<u32>>, Rc<Cell<i32>>) {
    let runs = Rc::new(Cell::new(0));
    let seen = Rc::new(Cell::new(0));
    let sink = Sink::new({
        let runs = Rc::clone(&runs);
        let seen = Rc::clone(&seen);
        move || {
            seen.set(read());
            runs.set(runs.get() + 1);
        }
    });
    (sink, runs, seen)
}

#[test]
fn the_first_run_is_synchronous_and_later_runs_wait_for_the_flush() {
    let (root, waker) = runtime();
    let source = root.with(|| RwSignal::new(1));
    let (sink, runs, seen) = root.with(|| counting(move || source.get()));
    assert_eq!((runs.get(), seen.get()), (1, 1));
    assert_eq!(waker.take(), 0);

    source.set(2);
    assert_eq!(runs.get(), 1, "a write runs nothing");
    assert_eq!(waker.take(), 1, "and asks for a frame");

    let outcome = flush();
    assert_eq!((runs.get(), seen.get()), (2, 2));
    assert!(!outcome.needs_another_frame);

    drop(sink);
    root.unmount();
}

#[test]
fn many_writes_between_two_flushes_cost_one_run() {
    let (root, _waker) = runtime();
    let source = root.with(|| RwSignal::new(0));
    let (sink, runs, seen) = root.with(|| counting(move || source.get()));

    for value in 1..=100 {
        source.set(value);
    }
    flush();
    assert_eq!((runs.get(), seen.get()), (2, 100));

    flush();
    assert_eq!(runs.get(), 2, "a settled sink does not run again");
    drop(sink);
    root.unmount();
}

#[test]
fn dropping_a_sink_frees_its_slot_and_its_queued_key_runs_nothing() {
    let (root, _waker) = runtime();
    let before = Sink::live();
    let source = root.with(|| RwSignal::new(0));
    let (first, first_runs, _) = root.with(|| counting(move || source.get()));
    assert_eq!(Sink::live(), before + 1);

    source.set(1);
    drop(first);
    assert_eq!(Sink::live(), before, "the slot is free");

    // The next sink takes the freed slot. The key queued for the first one names an older
    // generation of that slot, so it must not run the second one.
    let other = root.with(|| RwSignal::new(0));
    let (second, second_runs, _) = root.with(|| counting(move || other.get()));
    flush();
    assert_eq!(first_runs.get(), 1);
    assert_eq!(second_runs.get(), 1, "the stale key was skipped");

    drop(second);
    assert_eq!(Sink::live(), before);
    root.unmount();
}

#[test]
fn a_memo_that_recomputes_to_an_equal_value_does_not_run_the_sink() {
    let (root, _waker) = runtime();
    let (source, parity) = root.with(|| {
        let source = RwSignal::new(2);
        (source, Memo::new(move |_| source.get() % 2))
    });
    let (sink, runs, _) = root.with(|| counting(move || parity.get()));

    source.set(4);
    flush();
    assert_eq!(
        runs.get(),
        1,
        "the memo was asked first, and it did not change"
    );

    source.set(5);
    flush();
    assert_eq!(runs.get(), 2);
    drop(sink);
    root.unmount();
}

#[test]
fn a_diamond_runs_the_sink_once_with_both_sides_up_to_date() {
    let (root, _waker) = runtime();
    let (source, left, right) = root.with(|| {
        let source = RwSignal::new(1);
        (
            source,
            Memo::new(move |_| source.get() * 10),
            Memo::new(move |_| source.get() * 100),
        )
    });
    let (sink, runs, seen) = root.with(|| counting(move || left.get() + right.get()));
    assert_eq!(seen.get(), 110);

    source.set(2);
    flush();
    assert_eq!((runs.get(), seen.get()), (2, 220), "no glitch, one run");
    drop(sink);
    root.unmount();
}

#[test]
fn a_source_the_last_run_did_not_read_is_dropped() {
    let (root, _waker) = runtime();
    let (flag, a, b) = root.with(|| (RwSignal::new(true), RwSignal::new(1), RwSignal::new(2)));
    let (sink, runs, seen) =
        root.with(|| counting(move || if flag.get() { a.get() } else { b.get() }));

    flag.set(false);
    flush();
    assert_eq!((runs.get(), seen.get()), (2, 2));

    a.set(10);
    flush();
    assert_eq!(runs.get(), 2, "the sink no longer reads `a`");

    b.set(20);
    flush();
    assert_eq!((runs.get(), seen.get()), (3, 20));
    drop(sink);
    root.unmount();
}

#[test]
fn an_untracked_read_subscribes_to_nothing() {
    let (root, _waker) = runtime();
    let (tracked, untracked) = root.with(|| (RwSignal::new(1), RwSignal::new(1)));
    let (sink, runs, _) = root.with(|| counting(move || tracked.get() + untracked.get_untracked()));

    untracked.set(2);
    flush();
    assert_eq!(runs.get(), 1);

    tracked.set(2);
    flush();
    assert_eq!(runs.get(), 2);
    drop(sink);
    root.unmount();
}

#[test]
fn sinks_run_in_the_order_they_were_marked() {
    let (root, _waker) = runtime();
    let (first, second) = root.with(|| (RwSignal::new(0), RwSignal::new(0)));
    let order = Rc::new(RefCell::new(Vec::new()));
    let sinks: Vec<Sink> = [(first, 'a'), (second, 'b')]
        .into_iter()
        .map(|(source, name)| {
            let order = Rc::clone(&order);
            root.with(|| {
                Sink::new(move || {
                    source.get();
                    order.borrow_mut().push(name);
                })
            })
        })
        .collect();
    order.borrow_mut().clear();

    second.set(1);
    first.set(1);
    flush();
    assert_eq!(*order.borrow(), ['b', 'a']);
    drop(sinks);
    root.unmount();
}

#[test]
fn a_sink_that_writes_another_sinks_source_settles_in_one_flush() {
    let (root, waker) = runtime();
    let (source, middle) = root.with(|| (RwSignal::new(0), RwSignal::new(0)));
    let first = root.with(|| Sink::new(move || middle.set(source.get())));
    let (second, runs, seen) = root.with(|| counting(move || middle.get()));
    waker.take();

    source.set(7);
    let outcome = flush();
    assert_eq!((runs.get(), seen.get()), (2, 7));
    assert!(!outcome.needs_another_frame);
    assert_eq!(
        waker.take(),
        1,
        "only the write outside the flush asked for a frame"
    );
    drop((first, second));
    root.unmount();
}

#[test]
fn an_effect_that_drops_a_sink_runs_before_it() {
    let (root, _waker) = runtime();
    let source = root.with(|| RwSignal::new(0));
    let held: Rc<RefCell<Option<Sink>>> = Rc::new(RefCell::new(None));
    let inner_runs = Rc::new(Cell::new(0));

    // The effect subscribes first, as a branch does before the bindings it builds.
    let effect = root.with(|| {
        let held = Rc::clone(&held);
        let inner_runs = Rc::clone(&inner_runs);
        RenderEffect::new(move |_| {
            let value = source.get();
            let sink = (value == 0).then(|| {
                let inner_runs = Rc::clone(&inner_runs);
                Sink::new(move || {
                    // Would fail if it ran after the branch it belongs to was taken away.
                    assert_eq!(source.get(), 0, "a sink of a removed branch ran");
                    inner_runs.set(inner_runs.get() + 1);
                })
            });
            *held.borrow_mut() = sink;
        })
    });
    assert_eq!(inner_runs.get(), 1);

    source.set(1);
    flush();
    assert_eq!(inner_runs.get(), 1);
    assert!(held.borrow().is_none());
    drop(effect);
    root.unmount();
}

#[test]
fn a_sink_dropped_by_its_own_run_is_freed() {
    let (root, _waker) = runtime();
    let before = Sink::live();
    let source = root.with(|| RwSignal::new(0));
    let slot: Rc<RefCell<Option<Sink>>> = Rc::new(RefCell::new(None));
    let sink = root.with(|| {
        let slot = Rc::clone(&slot);
        Sink::new(move || {
            if source.get() == 1 {
                drop(slot.borrow_mut().take());
            }
        })
    });
    *slot.borrow_mut() = Some(sink);

    source.set(1);
    flush();
    assert!(slot.borrow().is_none());
    assert_eq!(Sink::live(), before);
    root.unmount();
}

#[test]
fn a_panicking_sink_stops_and_leaves_the_others_running() {
    let (root, _waker) = runtime();
    let source = root.with(|| RwSignal::new(0));
    let bad = root.with(|| {
        Sink::new(move || {
            if source.get() == 1 {
                panic!("a sink blew up");
            }
        })
    });
    let (good, runs, _) = root.with(|| counting(move || source.get()));

    source.set(1);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| ()));
    let flushed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(flush));
    std::panic::set_hook(previous);
    assert!(flushed.is_err(), "the panic reached the caller");

    flush();
    assert_eq!(
        runs.get(),
        2,
        "the next flush ran the sink queued behind it"
    );

    source.set(2);
    flush();
    assert_eq!(runs.get(), 3);
    drop((bad, good));
    root.unmount();
}

#[test]
fn two_sinks_writing_each_others_sources_are_cut_by_the_budget() {
    let (report, flushed) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (root, _waker) = runtime();
        let (left, right) = root.with(|| (RwSignal::new(0), RwSignal::new(0)));
        let first = root.with(|| Sink::new(move || right.set(left.get() + 1)));
        let second = root.with(|| Sink::new(move || left.set(right.get() + 1)));

        let _ = report.send(flush());
        drop((first, second));
        root.unmount();
    });

    let outcome = flushed
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("the flush returned instead of spinning on the cycle");
    assert!(outcome.budget_exhausted);
    assert!(outcome.needs_another_frame);
}

#[test]
fn a_write_from_another_thread_asks_for_a_frame() {
    let (root, waker) = runtime();
    let source = root.with(|| RwSignal::new(0));
    let (sink, runs, seen) = root.with(|| counting(move || source.get()));

    std::thread::spawn(move || source.set(5)).join().unwrap();
    assert_eq!(waker.take(), 1);
    assert_eq!(runs.get(), 1, "nothing ran off the UI thread");

    flush();
    assert_eq!((runs.get(), seen.get()), (2, 5));
    drop(sink);
    root.unmount();
}

#[test]
fn later_runs_see_the_contexts_of_the_first() {
    let (root, _waker) = runtime();
    let source = root.with(|| RwSignal::new(0));
    let seen = Rc::new(Cell::new(None));
    let sink = root.with(|| {
        crate::provide_context(41_u32);
        let seen = Rc::clone(&seen);
        Sink::new(move || {
            source.get();
            seen.set(crate::use_context::<u32>());
        })
    });

    source.set(1);
    Owner::new().with(flush);
    assert_eq!(seen.get(), Some(41));
    drop(sink);
    root.unmount();
}

#[test]
fn only_the_two_affected_rows_re_run() {
    let (root, _waker) = runtime();
    let _zone = crate::zone::enter_non_reactive_zone();
    let (source, selector, rows) = root.with(|| {
        let source = RwSignal::new(0_usize);
        (
            source,
            crate::Selector::new(move || source.get()),
            Scope::new(),
        )
    });
    let runs = Rc::new(Cell::new(0));
    let mut mounted = Vec::new();
    for key in 0..8 {
        let row = rows.mount();
        let sink = row.with(|| {
            let runs = Rc::clone(&runs);
            let selector = selector.clone();
            Sink::new(move || {
                selector.is_selected(&key);
                runs.set(runs.get() + 1);
            })
        });
        mounted.push((row, sink));
    }
    assert_eq!(runs.get(), 8);

    source.set(3);
    flush();
    assert_eq!(
        runs.get(),
        10,
        "the row that lost it and the row that gained it"
    );

    for (row, sink) in mounted {
        drop(sink);
        row.unmount();
    }
    assert_eq!(
        selector.watched_keys(),
        0,
        "the rows' scopes evicted their keys"
    );
    root.unmount();
}

#[test]
fn a_rerun_keeps_the_source_list() {
    let (root, _waker) = runtime();
    let source = root.with(|| RwSignal::new(0));
    let sink = root.with(|| {
        Sink::new(move || {
            source.get();
        })
    });
    let node = super::slots::node(sink.key).expect("the sink is live");
    assert_eq!(node.source_count(), 1);

    source.set(1);
    flush();
    assert_eq!(node.source_count(), 1, "the rerun subscribed again");
    drop(sink);
    root.unmount();
}

#[test]
fn a_dropped_sink_leaves_no_entry_in_its_sources() {
    let (root, _waker) = runtime();
    let (signal, memo) = root.with(|| {
        let signal = RwSignal::new(1);
        (signal, Memo::new(move |_| signal.get() * 2))
    });
    let sink = root.with(|| Sink::new(move || _ = signal.get() + memo.get()));
    let node = super::slots::node(sink.key).expect("the sink is live");
    assert_eq!(
        Arc::weak_count(&node),
        2,
        "the signal and the memo list the sink"
    );

    drop(sink);
    signal.set(2);
    flush();
    assert_eq!(Arc::weak_count(&node), 0, "no source lists the sink");
    root.unmount();
}

#[test]
fn a_write_after_a_drop_asks_for_no_frame() {
    let (root, waker) = runtime();
    let source = root.with(|| RwSignal::new(0));
    let sink = root.with(|| Sink::new(move || _ = source.get()));
    // The source keeps its entry for the sink until the next flush.
    drop(sink);
    waker.take();

    source.set(1);
    assert_eq!(waker.take(), 0, "the freed sink queues nothing");
    assert!(!flush().needs_another_frame);
    root.unmount();
}

#[test]
fn a_sink_dropped_by_its_own_run_leaves_no_entry_in_its_sources() {
    let (root, _waker) = runtime();
    let (source, late) = root.with(|| (RwSignal::new(0), RwSignal::new(0)));
    let slot: Rc<RefCell<Option<Sink>>> = Rc::new(RefCell::new(None));
    let sink = root.with(|| {
        let slot = Rc::clone(&slot);
        Sink::new(move || {
            if source.get() == 1 {
                drop(slot.borrow_mut().take());
                // A read after the drop subscribes the sink again.
                late.get();
            }
        })
    });
    let node = super::slots::node(sink.key).expect("the sink is live");
    *slot.borrow_mut() = Some(sink);

    source.set(1);
    flush();
    assert!(slot.borrow().is_none());
    assert_eq!(Arc::weak_count(&node), 0, "no source lists the sink");
    root.unmount();
}

#[test]
fn a_cleanup_a_run_registers_stays_until_the_owner_is_disposed_of() {
    let (root, _waker) = runtime();
    let source = root.with(|| RwSignal::new(0));
    let row = Mounted::new();
    let cleaned = Rc::new(Cell::new(0));
    let sink = row.with(|| {
        let cleaned = Rc::clone(&cleaned);
        Sink::new(move || {
            source.get();
            let cleaned = Rc::clone(&cleaned);
            crate::own::on_cleanup_local(move || cleaned.set(cleaned.get() + 1));
        })
    });

    source.set(1);
    flush();
    source.set(2);
    flush();
    assert_eq!(cleaned.get(), 0, "a rerun cleans up nothing");

    drop(sink);
    row.unmount();
    assert_eq!(cleaned.get(), 3, "the owner ran one cleanup for each run");
    root.unmount();
}

#[test]
fn in_one_round_the_effects_run_before_the_sinks() {
    let (root, _waker) = runtime();
    let source = root.with(|| RwSignal::new(0));
    let order = Rc::new(RefCell::new(Vec::new()));
    // The sink subscribes first, so wake order alone would run it first.
    let sink = root.with(|| {
        let order = Rc::clone(&order);
        Sink::new(move || order.borrow_mut().push(("sink", source.get())))
    });
    let effect = root.with(|| {
        let order = Rc::clone(&order);
        RenderEffect::new(move |_| order.borrow_mut().push(("effect", source.get())))
    });
    order.borrow_mut().clear();

    source.set(1);
    flush();
    assert_eq!(*order.borrow(), [("effect", 1), ("sink", 1)]);
    drop((sink, effect));
    root.unmount();
}

#[test]
fn a_sink_whose_owner_is_gone_runs_with_no_owner() {
    let (root, _waker) = runtime();
    let source = root.with(|| RwSignal::new(0));
    let row = Mounted::new();
    let seen = Rc::new(Cell::new(None));
    let sink = row.with(|| {
        crate::provide_context(41_u32);
        let seen = Rc::clone(&seen);
        Sink::new(move || {
            source.get();
            seen.set(crate::use_context::<u32>());
        })
    });
    assert_eq!(seen.get(), Some(41));

    row.unmount();
    source.set(1);
    flush();
    assert_eq!(
        seen.get(),
        None,
        "the run had no owner to resolve the context"
    );
    drop(sink);
    root.unmount();
}

#[test]
fn freed_sinks_leave_their_sources_last_run_first() {
    let (root, _waker) = runtime();
    let source = root.with(|| RwSignal::new(0));
    let sinks: Vec<Sink> = (0..64)
        .map(|_| root.with(|| Sink::new(move || _ = source.get())))
        .collect();
    let numbers: Vec<u32> = sinks
        .iter()
        .map(|sink| super::slots::run_number(sink.key))
        .collect();
    // The source lists the sinks in the order they ran. Removing the first entry first would move
    // every entry after it, once for each sink.
    super::slots::UNSUBSCRIBED.with_borrow_mut(Vec::clear);
    drop(sinks);
    flush();

    let order = super::slots::UNSUBSCRIBED.with_borrow_mut(std::mem::take);
    let mut expected = numbers;
    expected.reverse();
    assert_eq!(order, expected, "the last sink to run leaves first");
    root.unmount();
}
