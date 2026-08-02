//! The rule set, the restyle, and what one pass did.

use std::time::{Duration, Instant};

use rayon::ThreadPool;
use selectors::matching::QuirksMode;
use style::animation::DocumentAnimationSet;
use style::context::SharedStyleContext;
use style::global_style_data::GLOBAL_STYLE_DATA;
use style::shared_lock::{SharedRwLock, StylesheetGuards};
use style::stylesheets::{Origin, UrlExtraData};
use style::stylist::Stylist;
use style::traversal::DomTraversal;
use style::traversal_flags::TraversalFlags;
use zgui_dom::{Document, Node, NodeIndex, NodeKind, SnapshotStore};

use crate::support::sheets::{self, Errors};
use crate::support::traversal::{NoPainters, RecalcStyle, Restyled};
use crate::support::{device, pool, prefs};

/// What one restyle did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Pass {
    /// Whether the traversal ran at all.
    pub(crate) traversed: bool,
    /// How many elements the engine recorded as *re*styled.
    ///
    /// Set only for an element that already had a style, so a first pass over a fresh document
    /// reports none however much work it did. [`Pass::styled`] is the count that answers "how much".
    pub(crate) restyled: usize,
    /// How many elements came out with a computed style.
    pub(crate) styled: usize,
    /// How many came out with non-empty damage.
    pub(crate) damaged: usize,
    /// How many distinct workers ran at least one element.
    pub(crate) workers: usize,
    /// Which elements the traversal actually visited, by slot number, in visit order.
    pub(crate) visited: Vec<u32>,
    /// What the engine's own per-element data said about each of them.
    pub(crate) records: Vec<Restyled>,
    /// Time spent inside the engine.
    pub(crate) engine_time: Duration,
}

/// One style engine over one document.
pub(crate) struct Engine {
    /// The compiled rule set.
    stylist: Stylist,
    /// The lock shared with the document.
    lock: SharedRwLock,
    /// The base URL sheets are parsed against.
    url: UrlExtraData,
    /// What the parser dropped.
    errors: Errors,
    /// The element a worker is to panic on, if the failure policy is being exercised.
    panic_at: Option<u32>,
}

impl Engine {
    /// An engine for `document`, sharing its lock, with no stylesheets yet.
    pub(crate) fn new(document: &Document) -> Self {
        prefs::enable_css_features();
        Self {
            stylist: Stylist::new(device::device(1280.0, 800.0, 1.0), QuirksMode::NoQuirks),
            lock: document.store().lock().clone(),
            url: sheets::base_url(),
            errors: Errors::new(),
            panic_at: None,
        }
    }

    /// Makes the worker that reaches `node` panic, so the failure policy can be exercised.
    pub(crate) fn panic_in_worker_at(&mut self, node: NodeIndex) {
        self.panic_at = Some(node.get());
    }

    /// What the parser dropped.
    pub(crate) fn errors(&self) -> &Errors {
        &self.errors
    }

    /// Adds an author-origin sheet.
    pub(crate) fn add_author_sheet(&mut self, css: &str) {
        self.add_sheet(css, Origin::Author);
    }

    /// Adds a sheet at `origin`.
    pub(crate) fn add_sheet(&mut self, css: &str, origin: Origin) {
        let sheet = sheets::parse(css, origin, &self.lock, &self.url, &self.errors);
        let guard = self.lock.read();
        self.stylist.append_stylesheet(sheet, &guard);
    }

    /// Runs one restyle over `document`, on `pool` if one is given.
    ///
    /// Passing no pool runs the whole traversal on the calling thread, which is the comparison every
    /// parallel run is checked against.
    pub(crate) fn restyle(&mut self, document: &mut Document, pool: Option<&ThreadPool>) -> Pass {
        // Taken rather than borrowed: the restyle owns the records for its duration, and a change
        // made while it runs starts a fresh set that belongs to the next one.
        let mut snapshots = document.take_snapshots();
        let pass = pool::as_layout_thread(|| self.restyle_inner(document, &snapshots, pool));
        snapshots.clear(document.store());
        pass
    }

    /// The body of a restyle, with the calling thread already marked.
    fn restyle_inner(
        &mut self,
        document: &Document,
        snapshots: &SnapshotStore,
        pool: Option<&ThreadPool>,
    ) -> Pass {
        let Some(root) = document.root() else {
            return Pass::default();
        };
        let start = Instant::now();

        {
            let read = self.lock.read();
            let guards = StylesheetGuards {
                author: &read,
                ua_or_user: &read,
            };
            self.stylist
                .flush(&guards)
                .process_style(root, Some(snapshots.map()));
        }

        let read = self.lock.read();
        let guards = StylesheetGuards {
            author: &read,
            ua_or_user: &read,
        };
        let context = SharedStyleContext {
            traversal_flags: TraversalFlags::empty(),
            stylist: &self.stylist,
            options: GLOBAL_STYLE_DATA.options.clone(),
            guards,
            visited_styles_enabled: false,
            animations: DocumentAnimationSet::default(),
            current_time_for_animations: 0.0,
            snapshot_map: snapshots.map(),
            registered_speculative_painters: &NoPainters,
        };

        let token = <RecalcStyle<'_> as DomTraversal<Node<'_>>>::pre_traverse(root, &context);
        let traversed = token.should_traverse();
        let (workers, visited, records) = if traversed {
            let traverser = RecalcStyle {
                context,
                workers: std::sync::atomic::AtomicU32::new(0),
                visited: std::sync::Mutex::new(Vec::new()),
                restyled: std::sync::Mutex::new(Vec::new()),
                panic_at: self.panic_at,
            };
            // A worker that panics leaves per-element bookkeeping in a state no later traversal can
            // interpret, so the document is poisoned on the way out and the panic keeps going.
            document
                .guarded(|| style::driver::traverse_dom(&traverser, token, pool))
                .expect("the document is not poisoned");
            let workers = traverser
                .workers
                .load(std::sync::atomic::Ordering::Relaxed)
                .count_ones() as usize;
            let visited = traverser.visited.into_inner().expect("no worker panicked");
            let records = traverser.restyled.into_inner().expect("no worker panicked");
            (workers, visited, records)
        } else {
            drop(context);
            (0, Vec::new(), Vec::new())
        };
        drop(read);
        let engine_time = start.elapsed();

        let counted = count_and_clear(document);
        self.stylist.rule_tree().maybe_gc();
        Pass {
            traversed,
            restyled: counted.0,
            damaged: counted.1,
            styled: counted.2,
            workers,
            visited,
            records,
            engine_time,
        }
    }
}

/// Calls `visit` for every element of `document`, in document order.
pub(crate) fn for_each_element(document: &Document, mut visit: impl FnMut(Node<'_>)) {
    let mut stack = vec![document.document_index()];
    while let Some(index) = stack.pop() {
        let node = document.node(index);
        if node.kind() == NodeKind::Element {
            visit(node);
        }
        let mut child = document.store().core(index).last_child();
        while let Some(current) = child {
            stack.push(current);
            child = document.store().core(current).prev_sibling();
        }
    }
}

/// Counts restyled, damaged and styled elements, then clears the first two markers.
///
/// Clearing is the consumer's job: the engine leaves the flags set so an embedder can turn them into
/// its own invalidation, and a pass that does not clear them reports the previous pass's answer for
/// ever.
fn count_and_clear(document: &Document) -> (usize, usize, usize) {
    let mut restyled = 0;
    let mut damaged = 0;
    let mut styled = 0;
    for_each_element(document, |node| {
        let Some(mut data) = node.mutate_style_data() else {
            return;
        };
        if data.is_restyle() {
            restyled += 1;
        }
        if !data.damage.is_empty() {
            damaged += 1;
        }
        if data.styles.get_primary().is_some() {
            styled += 1;
        }
        data.clear_restyle_flags_and_damage();
    });
    (restyled, damaged, styled)
}

/// The computed value of four properties on every element, as a printable string.
///
/// Used to compare a parallel traversal against a sequential one: the point is not what the values
/// are, but that they are the same values.
pub(crate) fn computed_digest(document: &Document) -> Vec<String> {
    let mut out = Vec::new();
    for_each_element(document, |node| {
        let text = match node.primary_style() {
            Some(style) => format!(
                "{}:{:?}|{:?}|{:?}|{:?}",
                node.index().get(),
                style.get_inherited_text().clone_color(),
                style.get_box().clone_display(),
                style.get_font().clone_font_size(),
                style.get_border().clone_border_top_left_radius(),
            ),
            None => format!("{}:none", node.index().get()),
        };
        out.push(text);
    });
    out
}

/// Whether `element` has a style for `pseudo`.
pub(crate) fn has_pseudo_style(
    document: &Document,
    index: NodeIndex,
    pseudo: &style::selector_parser::PseudoElement,
) -> bool {
    document.node(index).pseudo_style(pseudo).is_some()
}
