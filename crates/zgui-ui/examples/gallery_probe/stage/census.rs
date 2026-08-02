//! Every node in the document, with where it is and what it says.
//!
//! A driver has to aim at things by name — "the button that says Outline", "the trigger inside the
//! dialog" — and the document offers no way to walk down from a parent to its children. What it
//! does offer is enough to walk *up*, to ask any handle for its geometry and its text, and to ask
//! whether one node contains another. So the census reconstructs the tree from the outside: it
//! enumerates the handles the node arena can have issued, keeps the ones that resolve inside this
//! document, and sorts them into document order through the same comparison the event system
//! resolves a path with.
//!
//! The enumeration is the one thing here that is not a question the public interface answers
//! directly. A handle packs an arena index, a generation and a domain, and the domain is read off a
//! handle that is already known — so the candidates are that domain, the low generations, and the
//! indices below a bound. A candidate that is not a live node in this document fails
//! [`ViewHost::contains`] and is dropped.

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::view::NodeId;

use crate::stage::handles::Handles;

/// How many arena slots in a row can come back empty before the scan accepts that it has passed
/// the last one. Indices are handed out from zero upwards, so a long enough run of nothing is the
/// end of the arena rather than a hole in it.
const RUN: u64 = 4_096;

/// The most slots to look at at all, so that a document which is somehow not dense cannot turn
/// this into an unbounded scan.
const SLOTS: u64 = 262_144;

/// How many generations one slot can have been through before this stops looking. A slot is
/// reissued each time an overlay closes and opens again, and the gallery is driven through a few
/// dozen of those.
const GENERATIONS: u64 = 128;

/// Where `node` is in the window, in device pixels from the surface's top-left corner.
///
/// [`ViewHost::window_box`](zgui::view::ViewHost::window_box) rather than a walk up the tree
/// summing origins and subtracting scroll offsets, and that is not a tidying. That arithmetic is
/// wrong for one kind of box and it is the kind every floating surface is made of: a
/// `position: fixed` box is in the viewport and takes no part of any ancestor's scroll, so a driver
/// doing the sum itself reports every menu and tooltip on a scrolled page as being thousands of
/// pixels above the window while the window is showing it beside its trigger. The engine's own
/// answer is the space the hit test resolves a pointer in, so a box and a pointer are in one space
/// by construction.
fn absolute(handles: &Handles, node: NodeId) -> Option<Rect<DevicePx, Device>> {
    handles.host.window_box(node)
}

/// One node of the document, as the outside can see it.
#[derive(Clone)]
pub(crate) struct Seen {
    /// The handle.
    pub(crate) id: NodeId,
    /// Where it is, in device pixels, when it produced a box at all.
    pub(crate) rect: Option<Rect<DevicePx, Device>>,
    /// Everything it says, its descendants included.
    pub(crate) text: String,
}

impl Seen {
    /// The middle of its box.
    pub(crate) fn centre(&self) -> Option<Point<DevicePx, Device>> {
        self.rect.map(|rect| {
            Point::new(
                DevicePx(rect.origin.x.0 + rect.size.width.0 / 2.0),
                DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
            )
        })
    }

    /// How much of the window it covers.
    pub(crate) fn area(&self) -> f32 {
        self.rect
            .map_or(0.0, |rect| rect.size.width.0 * rect.size.height.0)
    }
}

/// Everything in the document right now.
pub(crate) struct Census {
    /// The nodes, in document order.
    pub(crate) nodes: Vec<Seen>,
    /// The page's root first, then the overlay bands. A fold is something the page does, so which
    /// tree a box belongs to is what decides whether it can have folded anything. See
    /// [`Census::shown`].
    bands: Vec<NodeId>,
}

impl Census {
    /// Takes a census of `handles`' document.
    pub(crate) fn take(handles: &Handles) -> Self {
        let roots = handles.roots();
        let domain = handles.marker.as_u64() & 0xffff_0000_0000_0000;
        let mut nodes = Vec::new();
        let mut missed = 0;
        for index in 0..SLOTS {
            let mut hit = false;
            for generation in 1..=GENERATIONS {
                let bits = domain | (generation << 32) | index;
                let Some(id) = NodeId::from_u64(bits) else {
                    continue;
                };
                // Containment is the one question that is *total*: a handle naming a slot that was
                // never issued, or one whose generation has moved on, is answered `false` rather
                // than resolved. Every other question about a node — its box, its text — resolves
                // the handle and is only asked once this one has said the node is there.
                if !roots
                    .iter()
                    .any(|root| id == *root || handles.host.contains(*root, id))
                {
                    continue;
                }
                nodes.push(Seen {
                    id,
                    rect: absolute(handles, id),
                    text: handles.dom.text_content(id),
                });
                hit = true;
                break;
            }
            if hit {
                missed = 0;
            } else {
                missed += 1;
                if missed >= RUN {
                    break;
                }
            }
        }
        nodes.sort_by(|left, right| {
            if left.id == right.id {
                core::cmp::Ordering::Equal
            } else if handles.host.precedes(left.id, right.id) {
                core::cmp::Ordering::Less
            } else {
                core::cmp::Ordering::Greater
            }
        });
        // The overlay tree, whole: the layer roots and everything portalled onto one. Both are
        // boxes of no height — a layer is, and so is the box a component puts a menu in to place it
        // beside its trigger — and neither clips anything. A fold is something the page does.
        let mut bands: Vec<NodeId> = roots.clone();
        for node in &nodes {
            let floating = roots
                .iter()
                .skip(1)
                .any(|band| handles.host.contains(*band, node.id));
            if floating && !bands.contains(&node.id) {
                bands.push(node.id);
            }
        }
        Self { nodes, bands }
    }

    /// How many nodes there are.
    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Every node whose whole text is exactly `text`.
    pub(crate) fn saying(&self, text: &str) -> Vec<&Seen> {
        self.nodes.iter().filter(|node| node.text == text).collect()
    }

    /// The largest laid-out node whose whole text is exactly `text`, which is the control rather
    /// than the text inside it.
    ///
    /// What to *measure*. Several nested nodes carry one label — the text node, the control around
    /// it, and every wrapper up to the first that holds something else — and the control is the
    /// outermost of them that says nothing else. What to *aim at* is the other end of the same
    /// nesting: see [`Census::innermost`].
    pub(crate) fn control(&self, text: &str) -> Option<&Seen> {
        self.saying(text)
            .into_iter()
            .filter(|node| node.area() > 0.0)
            .max_by(|left, right| left.area().total_cmp(&right.area()))
    }

    /// The *smallest* laid-out node whose whole text is exactly `text`.
    ///
    /// What to aim a pointer at, and the distinction is not pedantry. The largest node saying a
    /// button's label is the row the button sits at one end of, and the centre of that row is the
    /// empty space beside the button: a click there lands on nothing, the control does not answer,
    /// and the run reports a broken component. The smallest is the innermost, and its centre lies
    /// inside every one of its ancestors — so it is on the control whichever of them is the control.
    pub(crate) fn innermost(&self, text: &str) -> Option<&Seen> {
        self.saying(text)
            .into_iter()
            .filter(|node| node.area() > 0.0)
            .min_by(|left, right| left.area().total_cmp(&right.area()))
    }

    /// The panel whose text begins with `title`, which is the card that heads it.
    pub(crate) fn panel(&self, title: &str) -> Option<&Seen> {
        // The heading is looked for by its *exact* text first, because one title can be a prefix
        // of another: asked for "Button", a search by prefix alone meets the "Button group and
        // Kbd" panel too, and if that one happens to be the smaller card it is the one answered
        // with — a panel full of things the caller never asked about.
        let heading = self
            .nodes
            .iter()
            .filter(|node| node.text == title && node.area() > 0.0)
            .min_by(|left, right| left.area().total_cmp(&right.area()))
            .or_else(|| {
                self.nodes
                    .iter()
                    .filter(|node| node.text.starts_with(title) && node.area() > 0.0)
                    .min_by(|left, right| left.area().total_cmp(&right.area()))
            })?;
        let inside = heading.rect?;
        // The panel is the smallest thing that begins with the title, says more than it, and
        // holds the heading's own box — that last condition is what keeps a same-prefix panel in
        // another column from answering, however small it is.
        self.nodes
            .iter()
            .filter(|node| {
                node.text.starts_with(title)
                    && node.area() > heading.area()
                    && node.text.len() > title.len()
                    && node.rect.is_some_and(|rect| {
                        rect.origin.x.0 <= inside.origin.x.0 + 0.5
                            && rect.origin.y.0 <= inside.origin.y.0 + 0.5
                            && rect.origin.x.0 + rect.size.width.0
                                >= inside.origin.x.0 + inside.size.width.0 - 0.5
                            && rect.origin.y.0 + rect.size.height.0
                                >= inside.origin.y.0 + inside.size.height.0 - 0.5
                    })
            })
            .min_by(|left, right| left.area().total_cmp(&right.area()))
    }

    /// Every node that lies inside `rect`, largest first.
    pub(crate) fn inside(&self, rect: Rect<DevicePx, Device>) -> Vec<&Seen> {
        self.nodes
            .iter()
            .filter(|node| {
                node.rect.is_some_and(|own| {
                    own.origin.x.0 >= rect.origin.x.0 - 0.5
                        && own.origin.y.0 >= rect.origin.y.0 - 0.5
                        && own.origin.x.0 + own.size.width.0
                            <= rect.origin.x.0 + rect.size.width.0 + 0.5
                        && own.origin.y.0 + own.size.height.0
                            <= rect.origin.y.0 + rect.size.height.0 + 0.5
                })
            })
            .collect()
    }

    /// The node with this handle, if the census saw it.
    pub(crate) fn node(&self, id: NodeId) -> Option<&Seen> {
        self.nodes.iter().find(|node| node.id == id)
    }

    /// Whether `node` is on the screen, rather than merely holding a box.
    ///
    /// A box of its own is not enough, and the difference is the whole of what a disclosure does.
    /// A folded section is clipped to no height at all by the element around it, and everything
    /// inside that element keeps its own box at its own full size — so "does this text have an
    /// area" answers `true` folded and unfolded alike, and a claim written on it cannot tell the
    /// two apart in either direction. What tells them apart is an ancestor closed to nothing,
    /// which is what this looks for.
    ///
    /// The question is asked of the page only. A floating surface is not on the page at all: it is
    /// portalled onto a band of its own, and a band is a box of no height that everything on it is
    /// *placed against* rather than clipped by. Asking it here would report every dialog, menu and
    /// tooltip in the window as not on the screen — which is the same defect this exists to avoid,
    /// pointed the other way.
    ///
    /// The overlay tree is therefore named rather than measured: the layer roots the document
    /// offers, and the root that holds them, are known handles, and a box of no height that is one
    /// of them has folded nothing.
    ///
    /// Naming them is what makes the answer independent of the window. Telling a band from a fold
    /// by width — a band being as wide as the window — is a question whose answer changes with the
    /// surface: a band one device pixel narrower than the window it spans, which is what rounding a
    /// fractional CSS width produces, reads as a folded section, and every menu and dialog in the
    /// run is then reported as not on the screen at that window size and at no other.
    pub(crate) fn shown(&self, handles: &Handles, node: &Seen) -> bool {
        if node.area() <= 0.0 {
            return false;
        }
        self.folding_ancestor(handles, node).is_none()
    }

    /// The ancestor that has folded `node` away, if one has.
    fn folding_ancestor(&self, handles: &Handles, node: &Seen) -> Option<&Seen> {
        self.nodes.iter().find(|other| {
            other.id != node.id
                && !self.bands.contains(&other.id)
                && other
                    .rect
                    .is_some_and(|rect| rect.size.height.0 <= 0.0 && rect.size.width.0 > 0.0)
                && handles.host.contains(other.id, node.id)
        })
    }

    /// Whether `node` is part of the page rather than of a floating surface.
    ///
    /// The question every search by *shape* has to ask first. The overlay bands sit at the window's
    /// own origin and carry boxes of their own — a toaster's viewport-anchored strip is a wide flat
    /// empty box at (0, 0) — and a panel that has been revealed to the top of the window contains
    /// them geometrically. So "the widest flat empty box in this panel" finds the band's box
    /// instead of the slider's track at any window size where the band's is the wider of the two,
    /// a press lands on the masthead, and the report is of a control that does not move.
    pub(crate) fn on_the_page(&self, node: &Seen) -> bool {
        !self.bands.contains(&node.id)
    }

    /// Whether anything whose whole text is exactly `text` is on the screen **and floating**.
    ///
    /// The question a claim about a menu, a dialog or a tooltip has to ask. A page says a great
    /// many things, and several of them are words a surface uses too: a command palette that lists
    /// *Settings* answers "is Settings on the screen" with a yes that has nothing to do with the
    /// menu whose item is also called that. So a menu that has just been closed reads as one that
    /// refused to close, for as long as any other component on the page happens to agree with it.
    pub(crate) fn floating(&self, handles: &Handles, text: &str) -> bool {
        self.nodes
            .iter()
            .filter(|node| node.text == text && self.bands.contains(&node.id))
            .any(|node| self.shown(handles, node))
    }

    /// Whether anything whose whole text is exactly `text` is on the screen.
    pub(crate) fn showing(&self, handles: &Handles, text: &str) -> bool {
        self.nodes
            .iter()
            .filter(|node| node.text == text)
            .any(|node| self.shown(handles, node))
    }

    /// Why the answer to [`Census::showing`] is what it is, node by node.
    ///
    /// A claim that something is not on the screen is worth nothing without this. There are three
    /// quite different reasons for it — nothing in the document says that at all, what says it
    /// produced no box, or what says it is inside something folded to nothing — and a report that
    /// cannot tell them apart sends a reader looking at the component when the fault is in the
    /// question.
    pub(crate) fn presence(&self, handles: &Handles, text: &str) -> String {
        let mut out = Vec::new();
        for node in self.nodes.iter().filter(|node| node.text == text) {
            let folded = self.folding_ancestor(handles, node);
            out.push(match (node.rect, folded) {
                (None, _) => "no box".to_owned(),
                (Some(rect), None) if node.area() > 0.0 => format!(
                    "{:.0},{:.0} {:.0}x{:.0}",
                    rect.origin.x.0, rect.origin.y.0, rect.size.width.0, rect.size.height.0
                ),
                (Some(rect), None) => format!(
                    "a box of no area at {:.0},{:.0}",
                    rect.origin.x.0, rect.origin.y.0
                ),
                (Some(_), Some(ancestor)) => {
                    let rect = ancestor.rect.unwrap_or(Rect::ZERO);
                    format!(
                        "folded away by an ancestor at {:.0},{:.0} {:.0}x{:.0} saying {:?}",
                        rect.origin.x.0,
                        rect.origin.y.0,
                        rect.size.width.0,
                        rect.size.height.0,
                        ancestor.text.chars().take(24).collect::<String>()
                    )
                }
            });
        }
        if out.is_empty() {
            return format!("nothing in the document says {text:?}");
        }
        format!("{text:?} is on {out:?}")
    }
}
