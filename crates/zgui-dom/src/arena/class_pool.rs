//! Every class name of every node, split once and interned once.
//!
//! `class="btn btn-primary large"` is one string in the source and three names in the matcher. It is
//! split and interned when it is written, and a node's record holds nothing but the range its names
//! occupy here. Selector matching therefore never splits a string, never allocates and never
//! compares characters: it compares interned handles inside a slice.
//!
//! Names are appended and never removed. Rewriting a node's classes appends a new run and leaves
//! the old one behind, which is the trade the pool exists to make: a class write is a rare event on
//! a hot path's data, and reclaiming its predecessor would mean either moving names other nodes are
//! pointing at or maintaining a free list over a structure whose whole value is that lookups are a
//! slice index.

use style::values::AtomIdent;

use crate::node::element::classes::ClassSpan;

/// The class names of one document.
#[derive(Default)]
pub struct ClassPool {
    /// Every run of names ever written, back to back.
    names: Vec<AtomIdent>,
}

impl ClassPool {
    /// An empty pool.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many names the pool holds, live runs and superseded ones alike.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the pool holds no names.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The names `span` covers.
    ///
    /// # Panics
    ///
    /// Panics if the span reaches past the end of the pool, which means it was built by something
    /// other than [`ClassPool::intern`] on this pool.
    pub fn resolve(&self, span: ClassSpan) -> &[AtomIdent] {
        &self.names[span.range()]
    }

    /// Appends `names` as one run and returns the span covering it.
    pub fn intern<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) -> ClassSpan {
        let start = self.names.len() as u32;
        for name in names {
            self.names.push(AtomIdent::from(name));
        }
        ClassSpan::new(start, self.names.len() as u32 - start)
    }

    /// Splits `text` on whitespace, appends the result as one run and returns its span.
    ///
    /// This is the form a `class` attribute arrives in.
    pub fn intern_attribute(&mut self, text: &str) -> ClassSpan {
        self.intern(text.split_ascii_whitespace())
    }
}

#[cfg(test)]
mod tests {
    use super::ClassPool;
    use crate::node::element::classes::ClassSpan;

    #[test]
    fn a_run_resolves_to_exactly_the_names_it_was_given() {
        let mut pool = ClassPool::new();
        assert!(pool.is_empty());
        let span = pool.intern(["btn", "btn-primary"]);
        assert_eq!(span.len(), 2);
        assert_eq!(
            pool.resolve(span)
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>(),
            vec!["btn", "btn-primary"]
        );
    }

    #[test]
    fn two_runs_do_not_overlap() {
        let mut pool = ClassPool::new();
        let first = pool.intern(["a"]);
        let second = pool.intern(["b", "c"]);
        assert_eq!(pool.resolve(first).len(), 1);
        assert_eq!(pool.resolve(second).len(), 2);
        assert_eq!(pool.resolve(second)[0].to_string(), "b");
        assert_eq!(pool.len(), 3);
    }

    #[test]
    fn an_attribute_is_split_on_whitespace_and_empty_runs_are_dropped() {
        let mut pool = ClassPool::new();
        let span = pool.intern_attribute("  btn   large  ");
        assert_eq!(span.len(), 2);
        assert_eq!(pool.resolve(span)[1].to_string(), "large");

        let empty = pool.intern_attribute("   ");
        assert!(empty.is_empty());
        assert_eq!(pool.resolve(empty), &[]);
    }

    #[test]
    fn the_empty_span_resolves_to_nothing_even_in_an_empty_pool() {
        let pool = ClassPool::new();
        assert_eq!(pool.resolve(ClassSpan::EMPTY), &[]);
    }
}
