# Contributing to zgui

`docs/planning/CONSTRAINTS.md` wins over everything else here. This file is about the mechanics.

## The definition of done

```
cargo xtask ci
```

green, on the pinned toolchain, from a clean checkout. It runs, in order:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo build --workspace`
4. `cargo test --workspace`
5. the five standing gates — `skips`, `budget`, `cadence`, `resize`, `workloads`
6. the wall-clock budgets and the performance ratchet, both in release
7. `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS=-D warnings`
8. the ledger checks

### The five standing gates

Each is a step of `ci` and a subcommand of its own, so it can be run alone while the thing it
covers is being changed.

| Gate | What it compares | Where the list lives |
|---|---|---|
| `cargo xtask skips` | Every counter declared `Group::Skip { done: … }` names a distinct counter of work *performed*, and has a call to `assert_non_vacuous` in some member's test code. A counter of avoided work reads zero when the stage is perfect and zero when the stage is gone, so a bound written against one alone is green from the day it is written. | `crates/zgui-profile/src/counter/table.rs`, checked by `xtask/src/ledger/check/skips/` |
| `cargo xtask budget` | A cache over its soft limit comes back under it within a bounded number of frames without thrashing, and no replayed range names a raster its cache has freed. | `xtask/src/budget/subject.rs` |
| `cargo xtask cadence` | An animation and the overscroll spring each get exactly one frame per refresh at 60, 75 and 240 Hz, on a headless window whose surface is put on an output with that stated rate. | `xtask/src/cadence/subject.rs` |
| `cargo xtask workloads` | Two of the reference workloads every later phase's claim is measured against — a one-control update against a whole-document one over 10 000 controls, and a fast wheel and a touchpad over a 100 000-row virtualised list. Every criterion is a **same-run ratio** and none is a duration. The gate also checks that each criterion a workload is registered for is still *stated*, so one deleted or renamed away cannot leave it green. Slopes in real units are printed beside the verdicts, marked advisory, and gate nothing. | `xtask/src/workloads/subject.rs` |
| `cargo xtask resize` | The resize slope — microseconds per box across four document sizes — as a **ratio** of the slope of a whole-document content change measured in the same run over the same documents, within 10% of the recorded ratio. The slope itself is printed and gates nothing: an absolute duration is keyed to the machine. | `crates/zgui-bench/src/bin/resize-slope/verdict.rs` |

The remaining reference workload has a gate of its own: the 42-step gallery script is `verify`. All
of them, their baselines and the machine those were taken on are indexed in
`docs/perf/reference-workloads.md`.

**The one workload that is deliberately not a gate.** A non-virtualised document of twenty thousand
rows is a diagnostic, run on demand:

```text
cargo run --release -p zgui-bench --bin unvirtualised-probe
```

It is the only document here that would make the scroll phase's slope look important, so wiring it
into `ci` would let a phase justify itself against a document no application would ship.
`xtask/src/workloads/subject.rs` carries a named test asserting it is not in the gate's list.

`budget` and `cadence` run assertions that live beside the code they are about. What the gate adds
is that the assertions are **named**: a target that has been emptied, or one whose assertion was
renamed away, fails the gate instead of passing quietly as one fewer test nobody counted. Renaming
one deliberately means editing the subject list in the same commit.

**What `cadence` does not cover.** Whether the frames the loop ran reached the device. A frame whose
picture is identical to the last one damages nothing and a renderer refuses it, so a window can run
a frame per refresh and present half as many; `scroll_cadence` closes most of that headlessly by
counting composed positions that *differ*, and what remains — the real swap chain, the real present,
a real output actually in the mode — needs a display server and a monitor. It is a runnable probe
rather than a gate:

```
cargo run --release -p zgui-bench --bin anim-cadence   -- dev.zgui.anim   10
cargo run --release -p zgui-bench --bin scroll-cadence -- dev.zgui.scroll bottom
```

Both open a window on the desktop they are run from and write their report to `ZGUI_CADENCE_OUT`.

**A new gate lands with the mutation that proves it fails.** Break what it guards, watch it fail,
put it back, and record the failure text in the commit message. A gate nobody has watched fail is
decoration, and one whose failure message does not tell a reader what to do is half a gate.

Nothing merges with the tree red, and nothing merges with a test `#[ignore]`d to make it green.
A test that cannot pass yet is a test that belongs to a later change. That one is `cargo xtask
ledger ignored` rather than a habit, because it is what every gate that reads the suite rests on: the
`skips` gate above establishes its proof by reading the source, so one attribute would leave it green
over an assertion nobody makes. There is no allowlist. A test that cannot run everywhere looks for
what it needs, prints on standard error that it did not find it, and returns — so what did not run is
visible where it happened rather than permanent in the file.

### What CI runs that `xtask ci` does not

Four checks are too slow or too narrow to belong in the local gate, so they are jobs of their own
in `.github/workflows/ci.yml`. Run them by hand before touching what they cover:

```
cargo miri test -p zgui-arena                 # the one unsafe block's aliasing claims
cargo test -p zgui-bits --features loom       # DirtyCell under every thread interleaving
cargo test -p zgui-profile --release --features counters
cargo xtask tsan                              # the threaded targets under ThreadSanitizer
```

`cargo xtask tsan` builds the threaded test targets and a rebuilt `std` with
`-Zsanitizer=thread`, runs them under `xtask/tsan-suppressions.txt`, and **runs a deliberate
data race in the same invocation**. It fails when that race is not reported, and when the
suppression file cannot be read. Both are ways a sanitiser run comes back green while watching
nothing, which is worse than not running it, because it reads as protection. Change what the
sanitiser covers by editing the subject list in `xtask/src/tsan/subject.rs`; the control has to
stay first.

The last one matters because a cargo feature that no job compiles is a feature that rots: the
default `cargo test --workspace` switches neither `loom` nor `counters` on, so both bodies of code
would otherwise be unreachable from every gate.

## The toolchain

`rust-toolchain.toml` pins the nightly. The pin moves deliberately, in its own commit, with CI
green on the new toolchain first. There is no stable-Rust build.

## The ledgers

`cargo xtask ledger` runs them all; `cargo xtask ledger <name>` runs one.

| Check | What it asserts |
|---|---|
| `engines` | No manifest names the reference implementation, and each external engine is named only by the crates permitted to name it. |
| `unsafe` | `#![forbid(unsafe_code)]` on every crate outside the allowlist, and every `unsafe impl Sync`/`Send` states its reason in a `// SAFETY:` comment directly above it. |
| `attribution` | Adapted code carries a licence header and a matching `NOTICE` row, in both directions, and no copyleft header appears in an Apache-2.0 crate. |
| `versions` | The exact-equality pins hold, wgpu resolves to one 29.x, the layout feature set is exactly the one the engine needs, the reactive graph has `effects` on and `nightly` off, and every external dependency is inherited from `[workspace.dependencies]`. |
| `topo` | The phase schedule in `docs/planning/PHASES.md` is a topological order of the real manifest graph. |
| `counters` | Every counter in the frame's counter block is incremented by some crate's shipped code, or listed in `xtask/src/ledger/check/counters/awaiting.rs` with the stage that will increment it — and a listed counter that has since acquired a producer is a violation, so the list only shrinks. A counter nothing increments reads zero forever, and every budget naming it passes while measuring nothing. |
| `skips` | Every counter declared `Group::Skip { done: … }` names a distinct counter of work performed and carries a non-vacuity assertion. Run on its own as `cargo xtask skips`. |
| `spikes` | Every `spikes/*` member carries a `# RETIRE: phase NN` header, and none outlives the phase that was supposed to delete it. |

`cargo xtask ledger --self-test` proves each check can fail: every check owns a `clean/` tree it
must accept and a `planted/` tree carrying the violation it exists to catch, under
`xtask/fixtures/<check>/`. **A new ledger rule lands with a planted fixture, or it does not
land.** A gate nobody has watched fail is decoration.

## Adding a crate

```
cargo xtask new-crate zgui-paint --layer L4
```

emits a manifest that inherits version, edition, licence and lints from the workspace, and a
crate root already carrying `#![deny(missing_docs)]` and `#![forbid(unsafe_code)]`. The workspace
globs `crates/*`, so the root manifest needs no edit. Name the crate in the phase that introduces
it in `docs/planning/PHASES.md`, or `cargo xtask ledger topo` will say so.

Dependencies are declared once, in the root `[workspace.dependencies]`, and inherited with
`foo.workspace = true`. A version requirement written in a member manifest is a ledger failure.

## Adapted code

Copying a shader function or a packing strategy from a compatibly licensed project is fine; the
file then starts with a header naming where it came from:

```
// DERIVED-FROM: the GPUI project, crates/gpui_wgpu/src/shaders.wgsl (Apache-2.0)
// Portions of this file are adapted from that work, licensed under the
// Apache License, Version 2.0, and have been modified for this renderer.
```

and gains a row in `NOTICE` under **Derived files**. Adapted code is rewritten to fit our own
primitives: the technique is what is being reused, never the surrounding type system.

## Performance budgets

The invalidation budget tests assert an exact amount of work — nodes visited, elements restyled,
primitives emitted — because a budget that drifts upward silently is not a budget.

**A budget assertion may only be raised in the commit that explains why.** That commit changes
the number, changes nothing else, and records the new value and the reason in the test's own doc
comment. Raising a budget as a side effect of a feature commit, or in a commit that also touches
the code under measurement, is not permitted: the number exists so that the cost of a change is
visible in review, and folding the two together is what makes it invisible.

Lowering a budget needs no ceremony.

### `MAX_DAMAGE`

`zgui_bits::MAX_DAMAGE` is how many disjoint rectangles a `DamageSet` holds before it merges to
stay inside itself. It is four, and it is a trade with a cost on both sides. Raising it lets a frame
that changed several unrelated places redraw each of them separately instead of redrawing their
bounding box — but every rectangle is its own render pass, with its own clear, its own scissor and
its own state changes, and those are paid for on every frame whether or not the extra precision
saved anything. Lowering it merges sooner, so a frame that touched two corners redraws the whole
surface.

**Changing it is a measurement, not an argument.** The commit that moves it changes nothing else,
and carries the scenario evidence that the new number is better: the run of `cargo xtask perf` at
the old value and at the new one, with the damage and pass numbers from both. "It seems like more
rectangles would help" is not evidence — the pass count is a per-frame cost paid by every document,
including the ones the extra rectangles do nothing for.

## Style

Small, focused, deeply nested modules. A file that would need section comments to separate
concerns is split into submodules instead.

Rustdoc on every public item, and it stands alone: it never refers to a plan, a phase, a review
or a discussion. It cross-references other items instead. Write it for a reader who has only the
published documentation.

Commit messages are concise and describe only what changed. No co-author trailer, no references
to plans or discussions.
