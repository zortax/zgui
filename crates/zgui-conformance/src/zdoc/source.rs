//! What a converted test is: a viewport, a style sheet and a tree.

/// One converted conformance test.
#[derive(Clone, Debug, PartialEq)]
pub struct Zdoc {
    /// The viewport the tree is laid out in, in CSS pixels.
    pub viewport: (f32, f32),
    /// The style sheet, as written.
    pub css: String,
    /// The single root element.
    pub root: Element,
}

impl Zdoc {
    /// The default viewport, used by a document that does not name one.
    ///
    /// Chosen to match the size a conformance suite's own reference viewport is written against,
    /// so a converted test that said nothing about its viewport is laid out the way its author
    /// meant it to be.
    pub const DEFAULT_VIEWPORT: (f32, f32) = (800.0, 600.0);
}

/// One element of a converted tree.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    /// The element's name, which selectors match on.
    pub name: String,
    /// Its classes, in source order.
    pub classes: Vec<String>,
    /// Its text content, which becomes a single text child.
    pub text: Option<String>,
    /// Its natural size, when its content comes from outside the document.
    pub replaced: Option<(f32, f32)>,
    /// Its children, in source order.
    pub children: Vec<Element>,
}

impl Element {
    /// An element with nothing but a name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// How many elements this one and its descendants are, together.
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Element::count).sum::<usize>()
    }
}
