//! Every node of the open document, with where it is in the window and what it says.
//!
//! The document offers no walk from a parent to its children, so the tree is reconstructed from the
//! outside: the handles the node arena can have issued are enumerated, the ones that resolve inside
//! this document are kept, and each is asked for its geometry and its text.

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::view::NodeId;

use crate::desktop::grab::Handles;

/// How many arena slots in a row can come back empty before the scan accepts it has passed the last
/// one. Indices are handed out from zero upwards, so a long enough run of nothing is the end.
const RUN: u64 = 2_048;

/// The most slots to look at, so a document that is somehow not dense cannot make this unbounded.
const SLOTS: u64 = 65_536;

/// How many generations one slot can have been through before this stops looking. A slot is
/// reissued each time an overlay closes and opens again.
const GENERATIONS: u64 = 64;

/// Where `node` is in the window, in device pixels from the surface's top-left corner.
///
/// [`ViewHost::window_box`](zgui::view::ViewHost::window_box) rather than a walk up the tree
/// summing origins, and that is not a tidying. The rule is not "add every ancestor's origin and
/// subtract every ancestor's scroll": a `position: fixed` box is in the viewport and takes no part
/// of any ancestor's scroll, so a fixture that did the arithmetic itself would report every
/// floating surface on a scrolled page as being far off the top of the window while the window
/// shows it exactly where it belongs — a measurement that disagrees with the picture. The engine's
/// own answer is the one the hit test resolves a pointer in, so a box and a pointer position read
/// here are in one space by construction.
pub fn absolute(handles: &Handles, node: NodeId) -> Option<Rect<DevicePx, Device>> {
    handles.host.window_box(node)
}

/// How many ancestors `node` has.
///
/// Bounded, so a cycle in the tree is a wrong answer rather than a hang.
fn depth_of(handles: &Handles, node: NodeId) -> usize {
    let mut walker = handles.dom.parent(node);
    let mut depth = 0;
    for _ in 0..256 {
        let Some(ancestor) = walker else { break };
        depth += 1;
        walker = handles.dom.parent(ancestor);
    }
    depth
}

/// One node of the document, as the outside can see it.
#[derive(Clone)]
pub struct Seen {
    /// The handle.
    pub id: NodeId,
    /// Where it is, in device pixels, when it produced a box at all.
    pub rect: Option<Rect<DevicePx, Device>>,
    /// Everything it says, its descendants included.
    pub text: String,
    /// How many ancestors it has, which is what tells an outer node from an inner one.
    pub depth: usize,
}

impl Seen {
    /// The middle of its box.
    pub fn centre(&self) -> Option<Point<DevicePx, Device>> {
        self.rect.map(|rect| {
            Point::new(
                DevicePx(rect.origin.x.0 + rect.size.width.0 / 2.0),
                DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
            )
        })
    }

    /// How much of the window it covers.
    pub fn area(&self) -> f32 {
        self.rect
            .map_or(0.0, |rect| rect.size.width.0 * rect.size.height.0)
    }
}

/// Everything in the document right now.
pub struct Census {
    /// The nodes.
    pub nodes: Vec<Seen>,
}

impl Census {
    /// Takes a census of `handles`' document.
    pub fn take(handles: &Handles) -> Self {
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
                // Containment is the one question that is total: a handle naming a slot that was
                // never issued is answered `false` rather than resolved.
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
                    depth: depth_of(handles, id),
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
        Self { nodes }
    }

    /// The *smallest* laid-out node whose whole text is exactly `text`.
    ///
    /// Smallest, not largest, and this is the whole of why a driver has to be careful. Several
    /// nodes share one label — the text node, the control around it, and every wrapper up to the
    /// first that holds something else — and they are nested, so the largest of them is a container
    /// the control sits in a corner of. Aiming at the centre of *that* is aiming at the empty space
    /// beside an intrinsically sized button, which looks exactly like a control that does not
    /// answer. The smallest is the innermost, and its centre is inside every one of its ancestors.
    pub fn control(&self, text: &str) -> Option<&Seen> {
        self.nodes
            .iter()
            .filter(|node| node.text == text && node.area() > 0.0)
            .min_by(|left, right| left.area().total_cmp(&right.area()))
    }

    /// The panel whose text begins with `title`, which is the card that heads it.
    ///
    /// Two steps, because several nodes' text begins with the title: the smallest is the heading's
    /// own text node, and the panel is the smallest thing that holds both the heading and whatever
    /// is under it.
    pub fn panel(&self, title: &str) -> Option<&Seen> {
        let smallest = self
            .nodes
            .iter()
            .filter(|node| node.text.starts_with(title) && node.area() > 0.0)
            .min_by(|left, right| left.area().total_cmp(&right.area()))?;
        self.nodes
            .iter()
            .filter(|node| {
                node.text.starts_with(title)
                    && node.area() > smallest.area()
                    && node.text.len() > title.len()
            })
            .min_by(|left, right| left.area().total_cmp(&right.area()))
    }

    /// Whether `text` is on the page: present, and not folded away by whatever holds it.
    ///
    /// Several nodes carry the same text — the text node, and every wrapper up to the first that
    /// holds something else — and a control that folds its contents away collapses the *outermost*
    /// of them while leaving the text node its own box. So the question is asked of the outermost:
    /// the one that contains all the others.
    pub fn shows(&self, text: &str) -> bool {
        let Some(outermost) = self.outermost(text) else {
            return false;
        };
        outermost.area() > 0.0
    }

    /// The node carrying exactly `text` that contains every other node carrying it.
    ///
    /// `None` when nothing says it. When several say it and none contains the rest — which a
    /// document can produce by saying the same thing twice — the first is answered, because the
    /// question a caller is asking is whether the text is on the page at all.
    pub fn outermost<'a>(&'a self, text: &str) -> Option<&'a Seen> {
        let matching: Vec<&Seen> = self.nodes.iter().filter(|node| node.text == text).collect();
        matching.iter().copied().min_by_key(|node| node.depth)
    }
}
