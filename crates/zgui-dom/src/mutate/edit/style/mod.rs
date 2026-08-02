//! Declarations attached to one element rather than to a rule.
//!
//! An element carries at most one block of its own declarations, and everything that writes one
//! writes into that block: the `style` text an author wrote, one declaration a view's binding
//! owns, one custom property a theme sets. One block is not a simplification — it is what the
//! cascade can see. The style engine reads an element's own declarations through a single hook,
//! so a second block held beside it would take part in no cascade at all and would be a set of
//! values that never reached the screen.
//!
//! Replacing one declaration therefore does not reparse the rest: the block is kept parsed, and a
//! write replaces the entry for that property in place. Only [`Edit::set_inline_style`], which is
//! given text rather than a property and a value, parses more than one declaration — and, like
//! setting the `style` attribute anywhere else, it replaces everything the element declared.
//!
//! # What this costs the style engine
//!
//! Nothing but a cascade. An element's own declarations cannot change what any selector matches,
//! so the engine is told to re-cascade that element and skip matching entirely — which is the
//! cheapest hint there is, and the reason an animating inline value is affordable at all. A custom
//! property additionally re-cascades the subtree, because a custom property is inherited.

mod parse;

use style::invalidation::element::restyle_hints::RestyleHint;
use style::properties::PropertyDeclarationBlock;
use style::shared_lock::SharedRwLock;
use zgui_interned::CustomPropertyName;

use crate::id::node_key::NodeIndex;
use crate::mutate::edit::Edit;
use crate::side::inline_style::StyleBlock;

impl Edit<'_> {
    /// Replaces every declaration `node` carries of its own with the ones `css` spells out.
    ///
    /// `None` removes them all. Declarations that do not parse are dropped and the rest survive,
    /// exactly as they would in a stylesheet.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn set_inline_style(&mut self, node: NodeIndex, css: Option<&str>) {
        let parsed = css.map(parse::attribute).filter(|block| !block.is_empty());
        let store = self.store();
        let key = store.key_of(node);
        let lock = store.lock().clone();
        let slot = store.columns_mut().inline_style.get_mut(key);
        if parsed.is_none() && slot.is_none() {
            return;
        }
        *slot = parsed.map(|block| wrapped(&lock, block));
        self.recascade(node);
    }

    /// Sets or removes one of `node`'s own declarations, leaving the others alone.
    ///
    /// Returns `false`, changing nothing, when `property` is not a property this build has or
    /// `value` does not parse for it. That is the answer a caller surfaces as a diagnostic: an
    /// unknown property is dropped here for the same reason it is dropped in a stylesheet, and
    /// silently doing nothing is what would make a misspelling invisible.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn set_style_property(
        &mut self,
        node: NodeIndex,
        property: &str,
        value: Option<&str>,
    ) -> bool {
        self.write_declaration(node, property, value).is_some()
    }

    /// Writes one declaration, reporting whether it landed and whether it changed anything.
    ///
    /// `None` is "there is no such declaration to make"; `Some(false)` is "the block already said
    /// that", which is the case no consumer may turn into style-engine work.
    fn write_declaration(
        &mut self,
        node: NodeIndex,
        property: &str,
        value: Option<&str>,
    ) -> Option<bool> {
        let id = parse::property_id(property)?;
        let store = self.store();
        let key = store.key_of(node);
        let lock = store.lock().clone();
        // A reference-count bump, taken so the block can be written through while the column is
        // not borrowed: the handle and the column name the same block, so the write lands once.
        let held: Option<StyleBlock> = store.columns_mut().inline_style.get_mut(key).clone();

        let (changed, fresh) = match (&held, value) {
            (None, None) => (false, None),
            (None, Some(value)) => {
                let mut block = PropertyDeclarationBlock::new();
                if !parse::set(&mut block, property, &id, value) {
                    return None;
                }
                (true, Some(wrapped(&lock, block)))
            }
            (Some(block), value) => {
                let mut guard = lock.write();
                let block = block.write_with(&mut guard);
                let changed = match value {
                    Some(value) => {
                        if !parse::set(block, property, &id, value) {
                            return None;
                        }
                        true
                    }
                    None => parse::remove(block, &id),
                };
                (changed, None)
            }
        };
        if let Some(fresh) = fresh {
            *self.store().columns_mut().inline_style.get_mut(key) = Some(fresh);
        }
        if changed {
            self.recascade(node);
        }
        Some(changed)
    }

    /// Sets or removes one custom property on `node`.
    ///
    /// A custom property is a declaration like any other, so it goes into the same block. It
    /// additionally re-cascades everything below `node`, because a custom property is inherited
    /// and an element that reads one is usually not the element that set it.
    ///
    /// Returns whether `value` parsed. A custom property's name always resolves, so `false` here
    /// means the value was not a valid one.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn set_custom_property(
        &mut self,
        node: NodeIndex,
        property: CustomPropertyName,
        value: Option<&str>,
    ) -> bool {
        let Some(changed) = self.write_declaration(node, &property.to_declaration(), value) else {
            return false;
        };
        if changed {
            let (store, batch) = self.parts();
            batch
                .hints
                .record(store, node, RestyleHint::RECASCADE_DESCENDANTS);
        }
        true
    }

    /// Tells the engine to re-cascade `node` without re-matching anything.
    fn recascade(&mut self, node: NodeIndex) {
        let (store, batch) = self.parts();
        batch
            .hints
            .record(store, node, RestyleHint::RESTYLE_STYLE_ATTRIBUTE);
    }
}

/// One block, wrapped in the document's lock and reference-counted.
fn wrapped(lock: &SharedRwLock, block: PropertyDeclarationBlock) -> StyleBlock {
    servo_arc::Arc::new(lock.wrap(block))
}

#[cfg(test)]
mod tests;
