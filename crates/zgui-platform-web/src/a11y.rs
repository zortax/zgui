//! The accessibility tree as elements of the page.
//!
//! A canvas is one opaque element to a screen reader. So each surface has a second tree beside
//! its canvas: one element for each node of the application's accessibility tree, with the node's
//! role, name and states, placed where the node is drawn. Nobody sees these elements. A screen
//! reader reads them, and what it does to one (a click, a move of focus) goes back to the
//! application as an action on the node.
//!
//! The tree costs work on every update, so it is built only on request. Until then the surface
//! holds one element, a button named "Enable accessibility", which is what a screen reader finds
//! first.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use accesskit::{Node, NodeId, TreeUpdate};
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, EventTarget, HtmlCanvasElement, HtmlElement};

use crate::aria;

/// The attribute that names the node an element stands for.
const NODE: &str = "data-zgui-node";

/// One node and the element that stands for it.
struct Held {
    /// The node, as last stated.
    node: Node,
    /// Its element.
    element: HtmlElement,
    /// The box the element was last given: left, top, width and height.
    placed: Cell<Option<[f64; 4]>>,
    /// Whether the element holds text of its own.
    has_text: Cell<bool>,
}

/// The elements that stand for one surface's accessibility tree.
pub(crate) struct Mirror {
    /// The element that holds the tree, placed over the canvas.
    container: HtmlElement,
    /// The button that asks for the tree, until something presses it.
    placeholder: Option<HtmlElement>,
    /// The nodes.
    nodes: HashMap<NodeId, Held>,
    /// The root node.
    root: Option<NodeId>,
    /// The node that has the application's focus.
    focus: Option<NodeId>,
}

impl Mirror {
    /// The mirror of `canvas`, holding only the button that asks for the tree.
    pub(crate) fn new(canvas: &HtmlCanvasElement) -> Option<Self> {
        let document = canvas.owner_document()?;
        let container = element(&document, "div")?;
        style(
            &container,
            &[
                ("position", "fixed"),
                ("overflow", "hidden"),
                ("pointer-events", "none"),
                ("margin", "0"),
                ("padding", "0"),
            ],
        );
        let placeholder = element(&document, "button")?;
        placeholder.set_text_content(Some("Enable accessibility"));
        style(
            &placeholder,
            &[
                ("position", "absolute"),
                ("left", "0"),
                ("top", "0"),
                ("width", "1px"),
                ("height", "1px"),
                ("opacity", "0"),
                ("pointer-events", "none"),
                ("border", "0"),
                ("padding", "0"),
            ],
        );
        container.append_child(&placeholder).ok()?;
        canvas.parent_node()?.append_child(&container).ok()?;
        let mirror = Self {
            container,
            placeholder: Some(placeholder),
            nodes: HashMap::new(),
            root: None,
            focus: None,
        };
        mirror.cover(canvas);
        Some(mirror)
    }

    /// The element that holds the tree.
    pub(crate) fn container(&self) -> &HtmlElement {
        &self.container
    }

    /// Whether the tree was asked for.
    pub(crate) fn is_active(&self) -> bool {
        self.placeholder.is_none()
    }

    /// Records that the tree was asked for. Returns `false` when it already was.
    pub(crate) fn activate(&mut self) -> bool {
        match self.placeholder.take() {
            Some(placeholder) => {
                placeholder.remove();
                true
            }
            None => false,
        }
    }

    /// Whether `target` is the button that asks for the tree.
    pub(crate) fn is_placeholder(&self, target: &EventTarget) -> bool {
        self.placeholder.as_ref().is_some_and(|placeholder| {
            let placeholder: &EventTarget = placeholder.as_ref();
            placeholder == target
        })
    }

    /// Whether `target` is the container or an element inside it.
    pub(crate) fn owns(&self, target: &EventTarget) -> bool {
        target
            .dyn_ref::<web_sys::Node>()
            .is_some_and(|node| self.container.contains(Some(node)))
    }

    /// The node the element `target` stands for.
    pub(crate) fn node_of(&self, target: &EventTarget) -> Option<NodeId> {
        target
            .dyn_ref::<Element>()?
            .get_attribute(NODE)?
            .parse::<u64>()
            .ok()
            .map(NodeId)
    }

    /// The node that has the application's focus.
    pub(crate) fn focus(&self) -> Option<NodeId> {
        self.focus
    }

    /// Places the container over `canvas`.
    pub(crate) fn cover(&self, canvas: &HtmlCanvasElement) {
        let bounds = canvas.get_bounding_client_rect();
        let pixels = |value: f64| format!("{value:.2}px");
        style(
            &self.container,
            &[
                ("left", &pixels(bounds.left())),
                ("top", &pixels(bounds.top())),
                ("width", &pixels(bounds.width())),
                ("height", &pixels(bounds.height())),
            ],
        );
    }

    /// Applies one update of the tree.
    pub(crate) fn apply(&mut self, update: TreeUpdate) {
        let Some(document) = self.container.owner_document() else {
            return;
        };
        if let Some(tree) = update.tree {
            self.root = Some(tree.root);
        }
        let mut changed = Vec::with_capacity(update.nodes.len());
        for (id, node) in update.nodes {
            let held = match self.nodes.remove(&id) {
                Some(mut held) => {
                    held.node = node;
                    held
                }
                None => {
                    let Some(element) = element(&document, "div") else {
                        continue;
                    };
                    let _ = element.set_attribute(NODE, &id.0.to_string());
                    style(
                        &element,
                        &[
                            ("position", "absolute"),
                            ("box-sizing", "border-box"),
                            ("margin", "0"),
                            ("padding", "0"),
                            ("border", "0"),
                            ("overflow", "hidden"),
                            ("background", "transparent"),
                            ("color", "transparent"),
                            ("white-space", "pre"),
                            ("user-select", "none"),
                            ("pointer-events", "none"),
                            ("outline", "none"),
                        ],
                    );
                    Held {
                        node,
                        element,
                        placed: Cell::new(None),
                        has_text: Cell::new(false),
                    }
                }
            };
            describe(&held);
            self.nodes.insert(id, held);
            changed.push(id);
        }
        for id in changed {
            self.order_children(id);
        }
        self.focus = Some(update.focus);
        let Some(root) = self.root else { return };
        if let Some(held) = self.nodes.get(&root)
            && held.element.parent_node().as_ref() != Some(self.container.as_ref())
        {
            let _ = self.container.append_child(&held.element);
        }
        self.forget_unreachable(root);
        self.place(root, (0.0, 0.0), (0.0, 0.0), true);
    }

    /// Puts the elements of a node's children inside its element, in the node's order.
    fn order_children(&self, id: NodeId) {
        let Some(held) = self.nodes.get(&id) else {
            return;
        };
        let wanted: Vec<&HtmlElement> = held
            .node
            .children()
            .iter()
            .filter_map(|child| self.nodes.get(child))
            .map(|child| &child.element)
            .collect();
        let present = held.element.children();
        let in_order = present.length() as usize == wanted.len()
            && wanted.iter().enumerate().all(|(index, element)| {
                let element: &Element = element.as_ref();
                present.item(index as u32).as_ref() == Some(element)
            });
        if in_order {
            return;
        }
        // An element that is appended moves, so appending each one leaves them in order. What
        // was there and is no longer a child goes.
        for index in (0..present.length()).rev() {
            if let Some(stale) = present.item(index)
                && !wanted.iter().any(|element| {
                    let element: &Element = element.as_ref();
                    *element == stale
                })
            {
                stale.remove();
            }
        }
        for element in wanted {
            let _ = held.element.append_child(element);
        }
    }

    /// Drops every node the root no longer reaches.
    fn forget_unreachable(&mut self, root: NodeId) {
        let mut reached = HashSet::new();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            if let Some(held) = self.nodes.get(&id)
                && reached.insert(id)
            {
                pending.extend_from_slice(held.node.children());
            }
        }
        self.nodes.retain(|id, held| {
            let kept = reached.contains(id);
            if !kept {
                held.element.remove();
            }
            kept
        });
    }

    /// Places the element of a node and of every node under it.
    ///
    /// `space` is where the coordinate space the node is stated in begins, and `parent` is where
    /// the element of its parent begins, both from the top left of the surface in CSS pixels. A
    /// node states its box in its own space, and a node with a transform moves the space for
    /// itself and for what is under it. The root's transform is the scale of the display, which
    /// CSS pixels already have.
    fn place(&self, id: NodeId, space: (f64, f64), parent: (f64, f64), root: bool) {
        let Some(held) = self.nodes.get(&id) else {
            return;
        };
        let space = match held.node.transform() {
            Some(transform) if !root => {
                let moved = transform.as_coeffs();
                (space.0 + moved[4], space.1 + moved[5])
            }
            _ => space,
        };
        let (origin, placed) = match held.node.bounds() {
            Some(bounds) => {
                let origin = (space.0 + bounds.x0, space.1 + bounds.y0);
                let placed = [
                    origin.0 - parent.0,
                    origin.1 - parent.1,
                    bounds.width().max(0.0),
                    bounds.height().max(0.0),
                ];
                (origin, placed)
            }
            None => (parent, [0.0; 4]),
        };
        if held.placed.replace(Some(placed)) != Some(placed) {
            let pixels = |value: f64| format!("{value:.2}px");
            style(
                &held.element,
                &[
                    ("left", &pixels(placed[0])),
                    ("top", &pixels(placed[1])),
                    ("width", &pixels(placed[2])),
                    ("height", &pixels(placed[3])),
                ],
            );
        }
        for child in held.node.children() {
            self.place(*child, space, origin, false);
        }
    }

    /// The element of the focused node, ready to take the keyboard, or `None` where the focus
    /// is on the root or on no node, which is the canvas's to hold.
    pub(crate) fn focus_target(&self) -> Option<HtmlElement> {
        let held = self
            .focus
            .filter(|focus| Some(*focus) != self.root)
            .and_then(|focus| self.nodes.get(&focus))?;
        let _ = held.element.set_attribute("tabindex", "-1");
        Some(held.element.clone())
    }
}

impl Drop for Mirror {
    fn drop(&mut self) {
        self.container.remove();
    }
}

/// Writes the role, the states and the text of a node onto its element.
fn describe(held: &Held) {
    let element = &held.element;
    let node = &held.node;
    match aria::element_role(node) {
        Some(role) => {
            let _ = element.set_attribute("role", role);
        }
        None => {
            let _ = element.remove_attribute("role");
        }
    }
    let states = aria::states(node);
    for name in aria::STATES {
        match states.iter().find(|(stated, _)| *stated == name) {
            Some((_, value)) => {
                let _ = element.set_attribute(name, value);
            }
            None => {
                let _ = element.remove_attribute(name);
            }
        }
    }
    // Text is written only into an element with no child elements: writing it replaces them.
    let text = aria::text(node).filter(|_| node.children().is_empty());
    if text.is_some() || held.has_text.get() {
        element.set_text_content(text);
        held.has_text.set(text.is_some());
    }
}

/// A new element of `tag`.
fn element(document: &Document, tag: &str) -> Option<HtmlElement> {
    document.create_element(tag).ok()?.dyn_into().ok()
}

/// Sets style properties on an element.
fn style(element: &HtmlElement, properties: &[(&str, &str)]) {
    let style = element.style();
    for (name, value) in properties {
        let _ = style.set_property(name, value);
    }
}
