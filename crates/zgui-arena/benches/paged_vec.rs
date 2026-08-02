//! What a sparse side table costs, measured where it is read most: on the key that is not there.
//!
//! A column that most values do not participate in is read for every value all the same, so the
//! absent read is the one that has to be cheap. It should come out at a bounds test and a
//! null-pointer test and nothing else.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use zgui_arena::{DomainId, Generation, Key, PAGE_LEN, PagedVec};

/// A stand-in for whatever the arena stores.
struct Value;

/// A key for a given slot.
fn key(index: u32) -> Key<Value> {
    Key::new(index, Generation::FIRST, DomainId::FIRST)
}

/// A table with one entry every `PAGE_LEN` slots, which is the sparse column's shape.
fn scattered(pages: u32) -> PagedVec<Key<Value>, u32> {
    let mut table = PagedVec::for_domain(DomainId::FIRST);
    for page in 0..pages {
        *table.get_mut(key(page * PAGE_LEN as u32 * 2)) = page;
    }
    table
}

fn benches(c: &mut Criterion) {
    let table = scattered(64);

    c.bench_function("paged_vec/absent", |b| {
        // Every one of these slots is on a page that was never written to.
        let mut index = PAGE_LEN as u32;
        b.iter(|| {
            index = index.wrapping_add(PAGE_LEN as u32 * 2);
            black_box(table.get(key(black_box(index))))
        });
    });

    c.bench_function("paged_vec/present", |b| {
        let mut index = 0_u32;
        b.iter(|| {
            index = (index + PAGE_LEN as u32 * 2) % (PAGE_LEN as u32 * 128);
            black_box(table.get(key(black_box(index))))
        });
    });

    c.bench_function("paged_vec/beyond_the_index", |b| {
        b.iter(|| black_box(table.get(key(black_box(u32::MAX)))));
    });
}

criterion_group!(paged_vec, benches);
criterion_main!(paged_vec);
