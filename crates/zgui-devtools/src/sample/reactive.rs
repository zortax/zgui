//! What the reactive side of the program is holding, as far as it can be asked.
//!
//! # What this can and cannot show
//!
//! It cannot show the graph. The edges — which effects subscribe to which signal — live in each
//! node's `SubscriberSet`, behind a `pub(crate)` field reached through a `pub(crate)` trait, and
//! the public `Source` and `Subscriber` traits only *mutate* those sets: `add_subscriber`,
//! `remove_subscriber`, `clear_subscribers`, and nothing that enumerates. So "what would re-run if
//! this signal changed" cannot be answered from outside `reactive_graph` at all, and pretending
//! otherwise by guessing would be worse than saying so.
//!
//! What *is* reachable is ownership, and ownership is where the bugs of this kind actually live.
//! Every signal, memo and effect belongs to the scope that was current when it was made, and every
//! component instance is one scope. So this counts scopes: how many instances of each component are
//! alive, how deep their scopes sit, and how many have ever been built.
//!
//! **Alive against built is the diagnostic.** A list whose rows come and go is supposed to keep its
//! live count flat while the built count climbs — it is disposing of what it replaces. A live count
//! that climbs with the built count is a view that is never freeing its scopes, which is the leak
//! that shows up later as a document nothing removes and an effect that fires for a row nobody can
//! see any more.

use zgui::view::instrument;

/// One component, and every instance of it that is alive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Live {
    /// The component's short name.
    pub(crate) name: String,
    /// Where it was declared.
    pub(crate) source: (&'static str, u32),
    /// How many instances of it are alive.
    pub(crate) alive: usize,
    /// How deep the shallowest of those scopes sits in the ownership tree.
    pub(crate) least: usize,
    /// How deep the deepest does.
    ///
    /// Kept beside the shallowest because the *spread* is what a runaway looks like: instances of
    /// one component at twenty different depths are instances nesting inside each other.
    pub(crate) most: usize,
}

/// What the reactive tab shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Reactive {
    /// Every component with a live instance, most instances first.
    pub(crate) components: Vec<Live>,
    /// How many component instances are alive.
    pub(crate) alive: usize,
    /// How many have been built since the program started.
    pub(crate) built: u64,
    /// The deepest live scope, in levels of ownership.
    pub(crate) deepest: usize,
    /// Whether this build records component boundaries at all.
    pub(crate) instrumented: bool,
}

/// Reads what is alive right now.
pub(crate) fn sample_reactive() -> Reactive {
    let live = instrument::live();
    let mut components: Vec<Live> = Vec::new();
    for tag in &live {
        let name = tag.name.rsplit("::").next().unwrap_or(tag.name);
        match components.iter_mut().find(|held| held.name == name) {
            Some(held) => {
                held.alive += 1;
                held.least = held.least.min(tag.scope);
                held.most = held.most.max(tag.scope);
            }
            None => components.push(Live {
                name: name.to_owned(),
                source: (tag.file, tag.line),
                alive: 1,
                least: tag.scope,
                most: tag.scope,
            }),
        }
    }
    // Most instances first, then by name, so the list is stable between samples that hold the same
    // components — an order that moved on its own would redraw the tab for nothing.
    components.sort_by(|left, right| {
        right
            .alive
            .cmp(&left.alive)
            .then_with(|| left.name.cmp(&right.name))
    });

    Reactive {
        deepest: live.iter().map(|tag| tag.scope).max().unwrap_or(0),
        alive: live.len(),
        built: instrument::created(),
        instrumented: instrument::is_recording(),
        components,
    }
}
